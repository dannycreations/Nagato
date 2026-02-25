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
    match first {
      b'+' => {
        if line.len() > 4 && line.starts_with(b"+++ ") {
          Ok(TokenKind::NewFile(unquote_path(&line[4..])))
        } else {
          Ok(TokenKind::Addition(&line[1..]))
        }
      }
      b'-' => {
        if line.len() > 4 && line.starts_with(b"--- ") {
          Ok(TokenKind::OldFile(unquote_path(&line[4..])))
        } else {
          Ok(TokenKind::Deletion(&line[1..]))
        }
      }
      b' ' => Ok(TokenKind::Context(&line[1..])),
      b'@' if line.len() >= 3 && line[1] == b'@' && line[2] == b' ' => {
        self.parse_hunk_header(line)
      }
      b'd' => self.parse_git_header(line),
      b'f' if line.starts_with(b"file ") => {
        let file = unquote_path(line[5..].trim());
        Ok(TokenKind::FileHeader(BinaryPaths {
          old_file: file.clone(),
          new_file: file,
        }))
      }
      b'G' if line == b"GIT binary patch" => {
        self.set_mode(LexerMode::Binary);
        Ok(TokenKind::GitBinaryPatchHeader)
      }
      b'i' if line.starts_with(b"index ") => self.parse_index_line(line),
      b'l' if line.starts_with(b"label ") => {
        Ok(TokenKind::Label(line[6..].trim_start()))
      }
      b'n' if line.starts_with(b"new ") => {
        self.parse_mode_rest(&line[4..], TokenKind::NewFileMode)
      }
      b'o' if line.starts_with(b"old ") => {
        self.parse_mode_rest(&line[4..], TokenKind::OldFileMode)
      }
      b'r' | b'c' => self.parse_rename_copy_line(line),
      b's' if line.starts_with(b"similarity index ") => {
        self.parse_percentage_token(&line[17..], TokenKind::Similarity)
      }
      b'B' if line.starts_with(b"Binary files ") => {
        self.parse_binary_files_line(line)
      }
      b'\\' if line == b"\\ No newline at end of file" => {
        Ok(TokenKind::NoNewline)
      }
      _ => Err(ErrorKind::UnexpectedLine),
    }
  }

  #[inline]
  fn parse_hunk_header(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    // Hunk headers are parsed by splitting the line into range and optional label components using byte-level patterns.
    let header = &line[3..];
    let (content, label_part) = match memmem::find(header, b" @@") {
      Some(idx) => (&header[..idx], &header[idx + 3..]),
      None => (header, &[][..]),
    };
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
    let label = if label.is_empty() { None } else { Some(label) };

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
      if let Some((old_file, new_file)) = split_diff_paths(&line[11..]) {
        Ok(TokenKind::FileHeader(BinaryPaths { old_file, new_file }))
      } else {
        Err(ErrorKind::InvalidFileHeader)
      }
    } else if line.starts_with(b"dissimilarity index ") {
      self.parse_percentage_token(&line[20..], TokenKind::Dissimilarity)
    } else if line.starts_with(b"deleted ") {
      self.parse_mode_rest(&line[8..], TokenKind::DeletedFileMode)
    } else {
      Err(ErrorKind::UnexpectedLine)
    }
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
    if line.starts_with(b"rename ") {
      if line.starts_with(b"rename from ") {
        Ok(TokenKind::RenameFrom(unquote_path(&line[12..])))
      } else if line.starts_with(b"rename to ") {
        Ok(TokenKind::RenameTo(unquote_path(&line[10..])))
      } else {
        Err(ErrorKind::UnexpectedLine)
      }
    } else if line.starts_with(b"copy ") {
      if line.starts_with(b"copy from ") {
        Ok(TokenKind::CopyFrom(unquote_path(&line[10..])))
      } else if line.starts_with(b"copy to ") {
        Ok(TokenKind::CopyTo(unquote_path(&line[8..])))
      } else {
        Err(ErrorKind::UnexpectedLine)
      }
    } else {
      Err(ErrorKind::UnexpectedLine)
    }
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
    rest
      .strip_prefix(b"file mode ")
      .or_else(|| rest.strip_prefix(b"mode "))
      .map(f)
      .ok_or(ErrorKind::InvalidFileMode)
  }

  #[inline]
  fn parse_percentage_token(
    &self,
    s: &[u8],
    f: impl FnOnce(u32) -> TokenKind<'a>,
  ) -> Result<TokenKind<'a>, ErrorKind> {
    s.strip_suffix(b"%")
      .and_then(|s| parse_int::<u32>(s, 10))
      .filter(|(_, rest)| rest.is_empty())
      .map(|(num, _)| f(num))
      .ok_or(ErrorKind::InvalidPercentage)
  }
}
