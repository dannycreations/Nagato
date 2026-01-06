use bstr::ByteSlice;
use memchr::memmem;
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

  /// Dispatches line parsing based on the first character.
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
    let header = &line[3..];
    let content_end = memmem::find(header, b" @@").unwrap_or(header.len());
    let content = &header[..content_end];
    let mut parts = content.fields();

    let old_range = parts
      .next()
      .and_then(|s: &[u8]| s.strip_prefix(b"-"))
      .ok_or(ErrorKind::MissingRange)?;
    let new_range = parts
      .next()
      .and_then(|s: &[u8]| s.strip_prefix(b"+"))
      .ok_or(ErrorKind::MissingRange)?;

    let label = if content_end + 3 < header.len() {
      let l = &header[content_end + 3..];
      let l = l.trim_start();
      if l.is_empty() {
        None
      } else {
        Some(l)
      }
    } else {
      None
    };

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
    let rest = &line[6..];
    let mut parts = rest.fields();
    let hashes_bytes = parts.next().ok_or(ErrorKind::InvalidIndexHeader)?;
    let (old_hash, new_hash) = hashes_bytes
      .split_once_str(b"..")
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
    let line_content = line[13..]
      .strip_suffix(b" differ")
      .ok_or(ErrorKind::InvalidBinaryFilesLine)?;
    let mut parts = line_content.split_str(b" and ");
    let old_file = parts.next().ok_or(ErrorKind::InvalidBinaryFilesLine)?;
    let new_file = parts.next().ok_or(ErrorKind::InvalidBinaryFilesLine)?;
    Ok(TokenKind::Binary {
      old_file: strip_git_prefix(old_file),
      new_file: strip_git_prefix(new_file),
    })
  }

  /// Helper to parse mode lines after the initial keyword prefix.
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
