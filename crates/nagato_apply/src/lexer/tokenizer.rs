use bstr::ByteSlice;
use nagato_core::{strip_git_prefix, ErrorKind};

use crate::{lexer::LexerMode, Lexer, TokenKind};

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
          Ok(TokenKind::NewFile(strip_git_prefix(rest)))
        } else {
          Ok(TokenKind::Addition(&line[1..]))
        }
      }
      b'-' => {
        if let Some(rest) = line.strip_prefix(b"--- ") {
          Ok(TokenKind::OldFile(strip_git_prefix(rest)))
        } else {
          Ok(TokenKind::Deletion(&line[1..]))
        }
      }
      b' ' => Ok(TokenKind::Context(&line[1..])),
      b'@' if line.starts_with(b"@@ ") => self.parse_hunk_header(line),
      b'd' => self.parse_git_header(line),
      b'f' if line.starts_with(b"file ") => {
        let file = strip_git_prefix(line[5..].trim());
        Ok(TokenKind::FileHeader {
          old_file: file,
          new_file: file,
        })
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
      let mut parts = rest.fields();
      let old_file = parts.next().map(strip_git_prefix);
      let new_file = parts.next().map(strip_git_prefix);

      if let (Some(old_file), Some(new_file)) = (old_file, new_file) {
        Ok(TokenKind::FileHeader { old_file, new_file })
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
      Ok(TokenKind::RenameFrom(rest))
    } else if let Some(rest) = line.strip_prefix(b"rename to ") {
      Ok(TokenKind::RenameTo(rest))
    } else if let Some(rest) = line.strip_prefix(b"copy from ") {
      Ok(TokenKind::CopyFrom(rest))
    } else if let Some(rest) = line.strip_prefix(b"copy to ") {
      Ok(TokenKind::CopyTo(rest))
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
    let (old_file, new_file) = line[13..]
      .strip_suffix(b" differ")
      .and_then(|s| s.split_once_str(b" and "))
      .ok_or(ErrorKind::InvalidBinaryFilesLine)?;
    Ok(TokenKind::Binary {
      old_file: strip_git_prefix(old_file),
      new_file: strip_git_prefix(new_file),
    })
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
