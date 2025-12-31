use bstr::ByteSlice;
use memchr::memmem;
use nagato_core::{strip_git_prefix, ErrorKind};

use crate::{lexer::LexerMode, Lexer, TokenKind};

impl<'a> Lexer<'a> {
  pub fn tokenize_binary(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.is_empty() {
      return Ok(TokenKind::Context(&[]));
    }

    // Helper to parse binary patch type lines (literal/delta)
    let binary_type = if let Some(rest) = line.strip_prefix(b"literal ") {
      Some((b"literal" as &[u8], rest))
    } else {
      line
        .strip_prefix(b"delta ")
        .map(|rest| (b"delta" as &[u8], rest))
    };

    if let Some((kind, size)) = binary_type {
      return Ok(TokenKind::BinaryPatchType { kind, size });
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
  pub fn tokenize_text(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.is_empty() {
      self.is_new_file_context = true;
      return Ok(TokenKind::Context(&[]));
    }

    match line.first() {
      Some(b'+') => self.parse_plus_line(line),
      Some(b'-') => self.parse_minus_line(line),
      Some(b' ') => {
        self.is_new_file_context = true;
        Ok(TokenKind::Context(&line[1..]))
      }
      Some(b'@') if line.starts_with(b"@@ ") => {
        self.parse_hunk_header(&line[3..])
      }
      Some(b'd') => self.parse_d_line(line),
      Some(b'f') => self.parse_f_line(line),
      Some(b'G') => self.parse_g_line(line),
      Some(b'i') if line.starts_with(b"index ") => {
        self.parse_index_line(&line[6..])
      }
      Some(b'l') if line.starts_with(b"label ") => {
        Ok(TokenKind::Label(&line[6..]))
      }
      Some(b'n') => self.parse_n_line(line),
      Some(b'o') => self.parse_o_line(line),
      Some(b'r') => self.parse_r_line(line),
      Some(b'c') => self.parse_c_line(line),
      Some(b's') if line.starts_with(b"similarity index ") => {
        Ok(TokenKind::Similarity(&line[17..]))
      }
      Some(b'B') => self.parse_b_line(line),
      _ => self.parse_non_keyword_line(line),
    }
  }

  fn parse_plus_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(rest) = line.strip_prefix(b"+++ ") {
      Ok(TokenKind::NewFile(strip_git_prefix(rest)))
    } else {
      self.is_new_file_context = true;
      Ok(TokenKind::Addition(&line[1..]))
    }
  }

  fn parse_minus_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(rest) = line.strip_prefix(b"--- ") {
      Ok(TokenKind::OldFile(strip_git_prefix(rest)))
    } else {
      self.is_new_file_context = false;
      Ok(TokenKind::Deletion(&line[1..]))
    }
  }

  fn parse_d_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(rest) = line.strip_prefix(b"diff --git ") {
      self.parse_file_header(rest)
    } else if let Some(rest) = line.strip_prefix(b"dissimilarity index ") {
      Ok(TokenKind::Dissimilarity(rest))
    } else {
      self
        .parse_mode(line, b"deleted ", TokenKind::DeletedFileMode)
        .or_else(|_| self.parse_non_keyword_line(line))
    }
  }

  fn parse_f_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(rest) = line.strip_prefix(b"file ") {
      let file = strip_git_prefix(rest.trim());
      Ok(TokenKind::FileHeader {
        old_file: file,
        new_file: file,
      })
    } else {
      self.parse_non_keyword_line(line)
    }
  }

  fn parse_g_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line == b"GIT binary patch" {
      self.mode = LexerMode::Binary;
      Ok(TokenKind::GitBinaryPatchHeader)
    } else {
      self.parse_non_keyword_line(line)
    }
  }

  fn parse_n_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    self
      .parse_mode(line, b"new ", TokenKind::NewFileMode)
      .or_else(|_| self.parse_non_keyword_line(line))
  }

  fn parse_o_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    self
      .parse_mode(line, b"old ", TokenKind::OldFileMode)
      .or_else(|_| self.parse_non_keyword_line(line))
  }

  fn parse_r_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(rest) = line.strip_prefix(b"rename from ") {
      Ok(TokenKind::RenameFrom(rest))
    } else if let Some(rest) = line.strip_prefix(b"rename to ") {
      Ok(TokenKind::RenameTo(rest))
    } else {
      self.parse_non_keyword_line(line)
    }
  }

  fn parse_c_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(rest) = line.strip_prefix(b"copy from ") {
      Ok(TokenKind::CopyFrom(rest))
    } else if let Some(rest) = line.strip_prefix(b"copy to ") {
      Ok(TokenKind::CopyTo(rest))
    } else {
      self.parse_non_keyword_line(line)
    }
  }

  fn parse_b_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(rest) = line.strip_prefix(b"Binary files ") {
      let line_content = rest
        .strip_suffix(b" differ")
        .ok_or(ErrorKind::InvalidBinaryFilesLine)?;
      let mut parts = line_content.split_str(b" and ");
      let old_file = parts.next().ok_or(ErrorKind::InvalidBinaryFilesLine)?;
      let new_file = parts.next().ok_or(ErrorKind::InvalidBinaryFilesLine)?;
      return Ok(TokenKind::Binary {
        old_file: strip_git_prefix(old_file),
        new_file: strip_git_prefix(new_file),
      });
    }
    self.parse_non_keyword_line(line)
  }

  pub fn parse_hunk_header(
    &mut self,
    header: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
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

  pub fn parse_file_header(
    &self,
    rest: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    let mut parts = rest.fields();
    let old_file = parts.next().map(strip_git_prefix);
    let new_file = parts.next().map(strip_git_prefix);

    if let (Some(old_file), Some(new_file)) = (old_file, new_file) {
      Ok(TokenKind::FileHeader { old_file, new_file })
    } else {
      Err(ErrorKind::InvalidFileHeader)
    }
  }

  pub fn parse_index_line(
    &self,
    rest: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
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

  pub fn parse_non_keyword_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    match line.first() {
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

  /// Helper to parse mode lines with various prefixes.
  fn parse_mode(
    &self,
    line: &'a [u8],
    prefix: &[u8],
    f: impl FnOnce(&'a [u8]) -> TokenKind<'a>,
  ) -> Result<TokenKind<'a>, ErrorKind> {
    line
      .strip_prefix(prefix)
      .and_then(|r| r.strip_prefix(b"file mode ").or(r.strip_prefix(b"mode ")))
      .map(f)
      .ok_or(ErrorKind::InvalidFileMode)
  }
}
