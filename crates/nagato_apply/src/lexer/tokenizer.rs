use bstr::ByteSlice;
use nagato_core::{split_diff_paths, unquote_path, ErrorKind};

use crate::{lexer::LexerMode, BinaryPaths, Lexer, TokenKind};

impl<'a> Lexer<'a> {
  #[inline]
  pub fn tokenize_binary(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.is_empty() {
      return Ok(TokenKind::Context(&[]));
    }

    // Fast path for binary data lines which usually start with base85 chars.
    if let Some(rest) = line.strip_prefix(b"literal ") {
      return Ok(TokenKind::BinaryPatchType {
        kind: b"literal",
        size: rest,
      });
    }
    if let Some(rest) = line.strip_prefix(b"delta ") {
      return Ok(TokenKind::BinaryPatchType {
        kind: b"delta",
        size: rest,
      });
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
      return Ok(TokenKind::Context(&[]));
    }

    match line[0] {
      b'+' => {
        if let Some(rest) = line.strip_prefix(b"+++ ") {
          Ok(TokenKind::NewFile(unquote_path(rest)))
        } else {
          Ok(TokenKind::Addition(&line[1..]))
        }
      }
      b'-' => {
        if let Some(rest) = line.strip_prefix(b"--- ") {
          Ok(TokenKind::OldFile(unquote_path(rest)))
        } else {
          Ok(TokenKind::Deletion(&line[1..]))
        }
      }
      b' ' => Ok(TokenKind::Context(&line[1..])),
      b'@' if line.starts_with(b"@@ ") => self.parse_hunk_header(line),
      b'd' => self.parse_git_header(line),
      b'f' if line.starts_with(b"file ") => {
        let file = unquote_path(line[5..].trim());
        Ok(TokenKind::FileHeader(Box::new(BinaryPaths {
          old_file: file.clone(),
          new_file: file,
        })))
      }
      b'G' if line == b"GIT binary patch" => {
        self.set_mode(LexerMode::Binary);
        Ok(TokenKind::GitBinaryPatchHeader)
      }
      b'i' if line.starts_with(b"index ") => self.parse_index_line(line),
      b'l' if line.starts_with(b"label ") => Ok(TokenKind::Label(&line[6..])),
      b'n' if line.starts_with(b"new ") => {
        self.parse_mode_rest(&line[4..], TokenKind::NewFileMode)
      }
      b'o' if line.starts_with(b"old ") => {
        self.parse_mode_rest(&line[4..], TokenKind::OldFileMode)
      }
      b'r' | b'c' => self.parse_rename_copy_line(line),
      b's' if line.starts_with(b"similarity index ") => {
        Ok(TokenKind::Similarity(&line[17..]))
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
    let (content, label_part) =
      header.split_once_str(b" @@").unwrap_or((header, &[]));
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
    if let Some(rest) = line.strip_prefix(b"diff --git ") {
      if let Some((old_file, new_file)) = split_diff_paths(rest) {
        Ok(TokenKind::FileHeader(Box::new(BinaryPaths {
          old_file,
          new_file,
        })))
      } else {
        Err(ErrorKind::InvalidFileHeader)
      }
    } else if let Some(rest) = line.strip_prefix(b"dissimilarity index ") {
      Ok(TokenKind::Dissimilarity(rest))
    } else if let Some(rest) = line.strip_prefix(b"deleted ") {
      self.parse_mode_rest(rest, TokenKind::DeletedFileMode)
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
    if let Some(rest) = line.strip_prefix(b"rename from ") {
      Ok(TokenKind::RenameFrom(unquote_path(rest)))
    } else if let Some(rest) = line.strip_prefix(b"rename to ") {
      Ok(TokenKind::RenameTo(unquote_path(rest)))
    } else if let Some(rest) = line.strip_prefix(b"copy from ") {
      Ok(TokenKind::CopyFrom(unquote_path(rest)))
    } else if let Some(rest) = line.strip_prefix(b"copy to ") {
      Ok(TokenKind::CopyTo(unquote_path(rest)))
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

    // This is a bit tricky if paths have " and " in them, but usually git quotes them.
    // If it's quoted, we should handle it.
    let (old_file, new_file) =
      split_and(rest).ok_or(ErrorKind::InvalidBinaryFilesLine)?;

    Ok(TokenKind::Binary(Box::new(BinaryPaths {
      old_file: unquote_path(old_file),
      new_file: unquote_path(new_file),
    })))
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
}

fn split_and(s: &[u8]) -> Option<(&[u8], &[u8])> {
  if s.is_empty() {
    return None;
  }
  if s[0] == b'"' {
    let mut i = 1;
    while i < s.len() {
      if s[i] == b'"' {
        let path = &s[..i + 1];
        let rest = s[i + 1..].trim();
        if let Some(rest) = rest.strip_prefix(b"and ") {
          return Some((path, rest.trim()));
        }
      }
      if s[i] == b'\\' && i + 1 < s.len() {
        i += 1;
      }
      i += 1;
    }
  } else {
    return s.split_once_str(b" and ");
  }
  None
}
