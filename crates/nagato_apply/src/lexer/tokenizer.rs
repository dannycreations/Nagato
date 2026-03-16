use bstr::ByteSlice;
use memchr::memmem;
use nagato_core::{
  next_path_pair, parse_int, split_diff_paths, unquote_path, ErrorKind,
};

use crate::{lexer::LexerMode, BinaryPaths, Lexer, TokenKind};

impl<'a> Lexer<'a> {
  #[inline]
  pub fn tokenize_binary(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.is_empty() {
      return Ok(TokenKind::Gap);
    }

    // Fast path for binary data lines which usually start with base85 chars.
    if line.len() > 8 {
      if line.starts_with(b"literal ") {
        return Ok(TokenKind::BinaryPatchType {
          kind: b"literal",
          size: &line[8..],
        });
      }
      if line.starts_with(b"delta ") {
        return Ok(TokenKind::BinaryPatchType {
          kind: b"delta",
          size: &line[6..],
        });
      }
    }

    if line.starts_with(b"diff --git")
      || line.starts_with(b"--- ")
      || line.starts_with(b"+++ ")
    {
      self.set_mode(LexerMode::Text);
      return self.tokenize_text(line);
    }

    Ok(TokenKind::BinaryData(line))
  }

  #[inline]
  pub fn tokenize_text(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.is_empty() {
      return Ok(TokenKind::Gap);
    }

    let first = line[0];
    if first == b'+' && line.starts_with(b"+++ ") {
      return Ok(TokenKind::NewFile(unquote_path(&line[4..])));
    }
    if first == b'+' {
      return Ok(TokenKind::Addition(&line[1..]));
    }

    if first == b'-' && line.starts_with(b"--- ") {
      return Ok(TokenKind::OldFile(unquote_path(&line[4..])));
    }
    if first == b'-' {
      return Ok(TokenKind::Deletion(&line[1..]));
    }

    if first == b' ' {
      return Ok(TokenKind::Context(&line[1..]));
    }

    if first == b'@' && line.starts_with(b"@@ ") {
      return self.parse_hunk_header(line);
    }

    if first == b'd'
      && (line.starts_with(b"diff ")
        || line.starts_with(b"dissimilarity ")
        || line.starts_with(b"deleted "))
    {
      return self.parse_git_header(line);
    }

    if first == b'f' && line.starts_with(b"file ") {
      let file = unquote_path(line[5..].trim());
      return Ok(TokenKind::FileHeader(BinaryPaths {
        old_file: file.clone(),
        new_file: file,
      }));
    }

    if first == b'G' && line == b"GIT binary patch" {
      self.set_mode(LexerMode::Binary);
      return Ok(TokenKind::GitBinaryPatchHeader);
    }

    if first == b'i' && line.starts_with(b"index ") {
      return self.parse_index_line(line);
    }

    if first == b'l' && line.starts_with(b"label ") {
      return Ok(TokenKind::Label(line[6..].trim_start()));
    }

    if first == b'n' && line.starts_with(b"new ") {
      return self.parse_mode_rest(&line[4..], TokenKind::NewFileMode);
    }

    if first == b'o' && line.starts_with(b"old ") {
      return self.parse_mode_rest(&line[4..], TokenKind::OldFileMode);
    }

    if first == b'r' || first == b'c' {
      return self.parse_rename_copy_line(line);
    }

    if first == b's' && line.starts_with(b"similarity index ") {
      return self.parse_percentage_token(&line[17..], TokenKind::Similarity);
    }

    if first == b'B' && line.starts_with(b"Binary files ") {
      return self.parse_binary_files_line(line);
    }

    if first == b'\\' && line == b"\\ No newline at end of file" {
      return Ok(TokenKind::NoNewline);
    }

    Err(ErrorKind::UnexpectedLine)
  }

  #[inline]
  fn parse_hunk_header(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    // Hunk headers are parsed by splitting the line into range and optional label components using byte-level patterns.
    let header = &line[3..];
    let Some(idx) = memmem::find(header, b" @@") else {
      let mut parts = header.fields();
      let old_range = parts
        .next()
        .and_then(|s| s.strip_prefix(b"-"))
        .ok_or(ErrorKind::MissingRange)?;
      let new_range = parts
        .next()
        .and_then(|s| s.strip_prefix(b"+"))
        .ok_or(ErrorKind::MissingRange)?;

      return Ok(TokenKind::HunkHeader {
        old_range,
        new_range,
        label: None,
      });
    };

    let content = &header[..idx];
    let label_part = &header[idx + 3..];
    let mut parts = content.fields();

    let old_range = parts
      .next()
      .and_then(|s| s.strip_prefix(b"-"))
      .ok_or(ErrorKind::MissingRange)?;
    let new_range = parts
      .next()
      .and_then(|s| s.strip_prefix(b"+"))
      .ok_or(ErrorKind::MissingRange)?;

    let label = label_part.trim_start();
    let label = (!label.is_empty()).then_some(label);

    Ok(TokenKind::HunkHeader {
      old_range,
      new_range,
      label,
    })
  }

  #[inline]
  fn parse_git_header(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.starts_with(b"diff --git ") {
      let paths =
        split_diff_paths(&line[11..]).ok_or(ErrorKind::InvalidFileHeader)?;
      let old_file = paths.0;
      let new_file = paths.1;
      return Ok(TokenKind::FileHeader(BinaryPaths { old_file, new_file }));
    }

    if line.starts_with(b"dissimilarity index ") {
      let rest = &line[20..];
      return self.parse_percentage_token(rest, TokenKind::Dissimilarity);
    }

    if line.starts_with(b"deleted ") {
      let rest = &line[8..];
      return self.parse_mode_rest(rest, TokenKind::DeletedFileMode);
    }

    Err(ErrorKind::UnexpectedLine)
  }

  #[inline]
  fn parse_index_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    // Index lines are processed by extracting the hash pair and optional mode from the space-delimited fields.
    let mut parts = line[6..].fields();
    let (old_hash, new_hash) = parts
      .next()
      .and_then(|s| s.split_once_str(b".."))
      .ok_or(ErrorKind::InvalidIndexHeader)?;
    let mode = parts.next();
    Ok(TokenKind::Index {
      old_hash,
      new_hash,
      mode,
    })
  }

  #[inline]
  fn parse_rename_copy_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.starts_with(b"rename from ") {
      return Ok(TokenKind::RenameFrom(unquote_path(&line[12..])));
    }
    if line.starts_with(b"rename to ") {
      return Ok(TokenKind::RenameTo(unquote_path(&line[10..])));
    }
    if line.starts_with(b"copy from ") {
      return Ok(TokenKind::CopyFrom(unquote_path(&line[10..])));
    }
    if line.starts_with(b"copy to ") {
      return Ok(TokenKind::CopyTo(unquote_path(&line[8..])));
    }

    Err(ErrorKind::UnexpectedLine)
  }

  #[inline]
  fn parse_binary_files_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    // Binary file markers are parsed by extracting the file paths from a standardized "Binary files ... differ" message using byte-level split operations.
    let rest = &line[13..];
    let rest = rest.strip_suffix(b" differ").unwrap_or(rest);

    let (old_file, new_file) =
      next_path_pair(rest, b"and ").ok_or(ErrorKind::InvalidBinaryFilesLine)?;

    Ok(TokenKind::Binary(BinaryPaths { old_file, new_file }))
  }

  #[inline]
  fn parse_mode_rest(
    &self,
    rest: &'a [u8],
    f: impl FnOnce(&'a [u8]) -> TokenKind<'a>,
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(mode) = rest.strip_prefix(b"file mode ") {
      return Ok(f(mode));
    }

    if let Some(mode) = rest.strip_prefix(b"mode ") {
      return Ok(f(mode));
    }

    Err(ErrorKind::InvalidFileMode)
  }

  #[inline]
  fn parse_percentage_token(
    &self,
    s: &[u8],
    f: impl FnOnce(u32) -> TokenKind<'a>,
  ) -> Result<TokenKind<'a>, ErrorKind> {
    let s = s.strip_suffix(b"%").ok_or(ErrorKind::InvalidPercentage)?;
    let (num, rest) =
      parse_int::<u32>(s, 10).ok_or(ErrorKind::InvalidPercentage)?;
    if rest.is_empty() {
      Ok(f(num))
    } else {
      Err(ErrorKind::InvalidPercentage)
    }
  }
}
