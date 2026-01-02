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
      self.mode = LexerMode::Text;
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
      self.is_new_file_context = true;
      return Ok(TokenKind::Context(&[]));
    }

    match line.first() {
      Some(b'+') => {
        if let Some(rest) = line.strip_prefix(b"+++ ") {
          Ok(TokenKind::NewFile(strip_git_prefix(rest)))
        } else {
          self.is_new_file_context = true;
          Ok(TokenKind::Addition(&line[1..]))
        }
      }
      Some(b'-') => {
        if let Some(rest) = line.strip_prefix(b"--- ") {
          Ok(TokenKind::OldFile(strip_git_prefix(rest)))
        } else {
          self.is_new_file_context = false;
          Ok(TokenKind::Deletion(&line[1..]))
        }
      }
      Some(b' ') => {
        self.is_new_file_context = true;
        Ok(TokenKind::Context(&line[1..]))
      }
      Some(b'@') if line.starts_with(b"@@ ") => {
        let header = &line[3..];
        self.is_new_file_context = false;
        let content_end = memmem::find(header, b" @@").unwrap_or(header.len());
        let content = &header[..content_end];
        let mut parts = content.fields();

        let old_range = parts
          .next()
          .and_then(|s: &[u8]| s.strip_prefix(b"-"))
          .ok_or(ErrorKind::MissingOldRange)?;
        let new_range = parts
          .next()
          .and_then(|s: &[u8]| s.strip_prefix(b"+"))
          .ok_or(ErrorKind::MissingNewRange)?;

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
      Some(b'd') => {
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
      Some(b'f') if line.starts_with(b"file ") => {
        let file = strip_git_prefix(line[5..].trim());
        Ok(TokenKind::FileHeader {
          old_file: file,
          new_file: file,
        })
      }
      Some(b'G') if line == b"GIT binary patch" => {
        self.mode = LexerMode::Binary;
        Ok(TokenKind::GitBinaryPatchHeader)
      }
      Some(b'i') if line.starts_with(b"index ") => {
        let rest = &line[6..];
        let mut parts = rest.fields();
        let hashes_bytes = parts.next().ok_or(ErrorKind::InvalidIndexLine)?;
        let (old_hash, new_hash) = hashes_bytes
          .split_once_str(b"..")
          .ok_or(ErrorKind::InvalidIndexHashRange)?;
        let mode = parts.next();
        Ok(TokenKind::Index {
          old_hash,
          new_hash,
          mode,
        })
      }
      Some(b'l') if line.starts_with(b"label ") => {
        Ok(TokenKind::Label(&line[6..]))
      }
      Some(b'n') if line.starts_with(b"new ") => {
        self.parse_mode_rest(&line[4..], TokenKind::NewFileMode)
      }
      Some(b'o') if line.starts_with(b"old ") => {
        self.parse_mode_rest(&line[4..], TokenKind::OldFileMode)
      }
      Some(b'r') => {
        if let Some(rest) = line.strip_prefix(b"rename from ") {
          Ok(TokenKind::RenameFrom(rest))
        } else if let Some(rest) = line.strip_prefix(b"rename to ") {
          Ok(TokenKind::RenameTo(rest))
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      Some(b'c') => {
        if let Some(rest) = line.strip_prefix(b"copy from ") {
          Ok(TokenKind::CopyFrom(rest))
        } else if let Some(rest) = line.strip_prefix(b"copy to ") {
          Ok(TokenKind::CopyTo(rest))
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      Some(b's') if line.starts_with(b"similarity index ") => {
        Ok(TokenKind::Similarity(&line[17..]))
      }
      Some(b'B') if line.starts_with(b"Binary files ") => {
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
      Some(b'\\') if line == b"\\ No newline at end of file" => {
        if self.is_new_file_context {
          Ok(TokenKind::NewFileNoNewline)
        } else {
          Ok(TokenKind::OldFileNoNewline)
        }
      }
      _ => Err(ErrorKind::UnexpectedLine),
    }
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
