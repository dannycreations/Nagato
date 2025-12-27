use bstr::ByteSlice;
use memchr::memmem;
use nagato_core::{parse_int, strip_git_prefix, Error, ErrorKind};

use crate::{BinaryKind, Lexer, LexerItem, TokenKind};

impl<'a> Lexer<'a> {
  pub fn parse_binary_line(
    &mut self,
    line: &'a [u8],
    line_num: u32,
  ) -> Option<Result<LexerItem<'a>, Error>> {
    if line.is_empty() {
      return Some(Ok(LexerItem {
        token: TokenKind::Context(&[]),
        line_num,
      }));
    }

    // Helper to parse binary patch type lines (literal/delta)
    let binary_type = if let Some(rest) = line.strip_prefix(b"literal ") {
      Some((BinaryKind::Literal, rest))
    } else {
      line
        .strip_prefix(b"delta ")
        .map(|rest| (BinaryKind::Delta, rest))
    };

    if let Some((kind, rest)) = binary_type {
      if let Some((size, _)) = parse_int::<u64>(rest, 10) {
        return Some(Ok(LexerItem {
          token: TokenKind::BinaryPatchType { kind, size },
          line_num,
        }));
      }
    }

    if line.starts_with(b"diff --git")
      || line.starts_with(b"--- ")
      || line.starts_with(b"+++ ")
    {
      self.is_in_binary_patch = false;
      // Re-parse current line as normal text
      // Note: We can't easily "push back" the line in this structure without
      // changing the iterator logic or recursion.
      // However, seeing "diff --git" inside binary patch mode means we exited it.
      // We should return the token for this line.
      let token_result = self.dispatch_line(line);
      Some(
        token_result
          .map(|token| LexerItem { token, line_num })
          .map_err(|kind| Error::with_line(kind, line_num)),
      )
    } else {
      Some(Ok(LexerItem {
        token: TokenKind::BinaryData(line),
        line_num,
      }))
    }
  }

  /// Dispatches line parsing based on the first character.
  pub fn dispatch_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    match line[0] {
      b'+' => {
        if let Some(rest) = line.strip_prefix(b"+++ ") {
          Ok(TokenKind::NewFile(strip_git_prefix(rest)))
        } else {
          self.is_new_file_context = true;
          Ok(TokenKind::Addition(&line[1..]))
        }
      }
      b'-' => {
        if let Some(rest) = line.strip_prefix(b"--- ") {
          Ok(TokenKind::OldFile(strip_git_prefix(rest)))
        } else {
          self.is_new_file_context = false;
          Ok(TokenKind::Deletion(&line[1..]))
        }
      }
      b' ' => {
        self.is_new_file_context = true;
        Ok(TokenKind::Context(&line[1..]))
      }
      b'@' if line.starts_with(b"@@ ") => self.parse_hunk_header(&line[3..]),
      b'd' if line.starts_with(b"diff --git ") => {
        self.parse_file_header(&line[11..])
      }
      b'd' if line.starts_with(b"deleted ") => {
        if let Some(rest) = self.parse_mode_rest(line, b"deleted ") {
          parse_int::<u32>(rest, 8)
            .map(|(m, _)| TokenKind::DeletedFileMode(m))
            .ok_or(ErrorKind::InvalidFileMode)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'd' if line.starts_with(b"dissimilarity index ") => self
        .parse_percentage(&line[20..])
        .map(TokenKind::Dissimilarity),
      b'f' if line.starts_with(b"file ") => {
        let file = strip_git_prefix(line[5..].trim());
        Ok(TokenKind::FileHeader {
          old_file: file,
          new_file: file,
        })
      }
      b'G' if line == b"GIT binary patch" => {
        self.is_in_binary_patch = true;
        Ok(TokenKind::GitBinaryPatchHeader)
      }
      b'i' if line.starts_with(b"index ") => self.parse_index_line(&line[6..]),
      b'n' if line.starts_with(b"new ") => {
        if let Some(rest) = self.parse_mode_rest(line, b"new ") {
          parse_int::<u32>(rest, 8)
            .map(|(m, _)| TokenKind::NewFileMode(m))
            .ok_or(ErrorKind::InvalidFileMode)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'o' if line.starts_with(b"old ") => {
        if let Some(rest) = self.parse_mode_rest(line, b"old ") {
          parse_int::<u32>(rest, 8)
            .map(|(m, _)| TokenKind::OldFileMode(m))
            .ok_or(ErrorKind::InvalidFileMode)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'r' if line.starts_with(b"rename from ") => {
        Ok(TokenKind::RenameFrom(&line[12..]))
      }
      b'r' if line.starts_with(b"rename to ") => {
        Ok(TokenKind::RenameTo(&line[10..]))
      }
      b'c' if line.starts_with(b"copy from ") => {
        Ok(TokenKind::CopyFrom(&line[10..]))
      }
      b'c' if line.starts_with(b"copy to ") => {
        Ok(TokenKind::CopyTo(&line[8..]))
      }
      b's' if line.starts_with(b"similarity index ") => self
        .parse_percentage(&line[17..])
        .map(TokenKind::Similarity),
      b'B' if line.starts_with(b"Binary files ") => {
        if let Some(line_content) = line[13..].strip_suffix(b" differ") {
          let mut parts = line_content.split_str(b" and ");
          if let (Some(old_file), Some(new_file)) = (parts.next(), parts.next())
          {
            Ok(TokenKind::Binary {
              old_file: strip_git_prefix(old_file),
              new_file: strip_git_prefix(new_file),
            })
          } else {
            Err(ErrorKind::InvalidBinaryFilesLine)
          }
        } else {
          Err(ErrorKind::InvalidBinaryFilesLine)
        }
      }
      _ => self.parse_non_keyword_line(line),
    }
  }

  pub fn parse_range(
    &self,
    range_bytes: &[u8],
  ) -> Result<(u32, u32), ErrorKind> {
    let (line, rest) = parse_int::<u32>(range_bytes, 10)
      .ok_or(ErrorKind::InvalidHunkRangeLine)?;

    let span = if let Some(rest) = rest.strip_prefix(b",") {
      let (span, rest) =
        parse_int::<u32>(rest, 10).ok_or(ErrorKind::InvalidHunkRangeSpan)?;
      if !rest.is_empty() {
        return Err(ErrorKind::InvalidHunkRangeSpan);
      }
      span
    } else if rest.is_empty() {
      1
    } else {
      return Err(ErrorKind::InvalidHunkRangeLine);
    };

    Ok((line, span))
  }

  pub fn parse_hunk_header(
    &mut self,
    header: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    self.is_new_file_context = false;
    let content_end = memmem::find(header, b" @@").unwrap_or(header.len());
    let content = &header[..content_end];
    let mut parts = content.fields();

    let old_range_bytes = parts
      .next()
      .and_then(|s: &[u8]| s.strip_prefix(b"-"))
      .ok_or(ErrorKind::MissingOldRange)?;
    let new_range_bytes = parts
      .next()
      .and_then(|s: &[u8]| s.strip_prefix(b"+"))
      .ok_or(ErrorKind::MissingNewRange)?;

    let (old_line, old_span) = self.parse_range(old_range_bytes)?;
    let (new_line, new_span) = self.parse_range(new_range_bytes)?;

    Ok(TokenKind::HunkHeader {
      old_line,
      old_span,
      new_line,
      new_span,
    })
  }

  pub fn parse_percentage(&self, s: &[u8]) -> Result<u32, ErrorKind> {
    let s = s.strip_suffix(b"%").ok_or(ErrorKind::InvalidPercentage)?;
    let (num, rest) =
      parse_int::<u32>(s, 10).ok_or(ErrorKind::InvalidPercentage)?;
    if !rest.is_empty() {
      return Err(ErrorKind::InvalidPercentage);
    }
    Ok(num)
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
    let mode = parts
      .next()
      .and_then(|m| parse_int::<u32>(m, 8).map(|(v, _)| v));
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
  fn parse_mode_rest(&self, line: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    line
      .strip_prefix(prefix)
      .and_then(|r| r.strip_prefix(b"file mode ").or(r.strip_prefix(b"mode ")))
  }
}
