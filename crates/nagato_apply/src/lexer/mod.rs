use bstr::ByteSlice;
use memchr::memmem;
use nagato_core::error::{Error, ErrorKind};

pub mod token;
pub mod utils;

pub use token::TokenKind;
use utils::{parse_int, strip_git_prefix};

use crate::models::binary::BinaryPatchKind;

#[derive(Debug, Clone, PartialEq)]
pub struct LexerItem<'a> {
  pub token: TokenKind<'a>,
  pub line_num: u32,
}

#[doc(hidden)]
pub struct Lexer<'a> {
  input: &'a [u8],
  pos: usize,
  line_num: u32,
  last_line_was_new_file: bool,
  in_binary_patch: bool,
}

impl<'a> Lexer<'a> {
  #[doc(hidden)]
  pub fn new(input: &'a [u8]) -> Self {
    Lexer {
      input,
      pos: 0,
      line_num: 0,
      last_line_was_new_file: false,
      in_binary_patch: false,
    }
  }

  fn parse_line(&mut self) -> Option<Result<LexerItem<'a>, Error>> {
    let line = self.next_line()?;
    let line_num = self.line_num;

    if self.in_binary_patch {
      if line.is_empty() {
        return Some(Ok(LexerItem {
          token: TokenKind::Context(&[]),
          line_num,
        }));
      }

      if let Some(rest) = line.strip_prefix(b"literal ") {
        if let Some((size, _)) = parse_int::<u64>(rest, 10) {
          return Some(Ok(LexerItem {
            token: TokenKind::BinaryPatchType {
              kind: BinaryPatchKind::Literal,
              size,
            },
            line_num,
          }));
        }
      } else if let Some(rest) = line.strip_prefix(b"delta ") {
        if let Some((size, _)) = parse_int::<u64>(rest, 10) {
          return Some(Ok(LexerItem {
            token: TokenKind::BinaryPatchType {
              kind: BinaryPatchKind::Delta,
              size,
            },
            line_num,
          }));
        }
      }

      if line.starts_with(b"diff --git")
        || line.starts_with(b"--- ")
        || line.starts_with(b"+++ ")
      {
        self.in_binary_patch = false;
      } else {
        return Some(Ok(LexerItem {
          token: TokenKind::BinaryData(line),
          line_num,
        }));
      }
    }

    if line.is_empty() {
      self.last_line_was_new_file = true;
      return Some(Ok(LexerItem {
        token: TokenKind::Context(&[]),
        line_num,
      }));
    }

    let token_result: Result<TokenKind, ErrorKind> = match line[0] {
      b'+' => {
        if let Some(rest) = line.strip_prefix(b"+++ ") {
          Ok(TokenKind::NewFile(strip_git_prefix(rest)))
        } else {
          self.last_line_was_new_file = true;
          Ok(TokenKind::Addition(&line[1..]))
        }
      }
      b'-' => {
        if let Some(rest) = line.strip_prefix(b"--- ") {
          Ok(TokenKind::OldFile(strip_git_prefix(rest)))
        } else {
          self.last_line_was_new_file = false;
          Ok(TokenKind::Deletion(&line[1..]))
        }
      }
      b' ' => {
        self.last_line_was_new_file = true;
        Ok(TokenKind::Context(&line[1..]))
      }
      b'@' => {
        if let Some(rest) = line.strip_prefix(b"@@ ") {
          self.parse_hunk_header(rest)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'd' => {
        if let Some(rest) = line.strip_prefix(b"diff --git ") {
          self.parse_file_header(rest)
        } else if let Some(rest) = line.strip_prefix(b"deleted file mode ") {
          parse_int::<u32>(rest, 8)
            .map(|(m, _)| TokenKind::DeletedFileMode(m))
            .ok_or(ErrorKind::InvalidFileMode)
        } else if let Some(rest) = line.strip_prefix(b"deleted mode ") {
          parse_int::<u32>(rest, 8)
            .map(|(m, _)| TokenKind::DeletedFileMode(m))
            .ok_or(ErrorKind::InvalidFileMode)
        } else if let Some(rest) = line.strip_prefix(b"dissimilarity index ") {
          self.parse_percentage(rest).map(TokenKind::Dissimilarity)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'f' => {
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
      b'G' => {
        if line == b"GIT binary patch" {
          self.in_binary_patch = true;
          Ok(TokenKind::GitBinaryPatchHeader)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'i' => {
        if let Some(rest) = line.strip_prefix(b"index ") {
          self.parse_index_line(rest)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'n' => {
        if let Some(rest) = line.strip_prefix(b"new file mode ") {
          parse_int::<u32>(rest, 8)
            .map(|(m, _)| TokenKind::NewFileMode(m))
            .ok_or(ErrorKind::InvalidFileMode)
        } else if let Some(rest) = line.strip_prefix(b"new mode ") {
          parse_int::<u32>(rest, 8)
            .map(|(m, _)| TokenKind::NewFileMode(m))
            .ok_or(ErrorKind::InvalidFileMode)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'o' => {
        if let Some(rest) = line.strip_prefix(b"old file mode ") {
          parse_int::<u32>(rest, 8)
            .map(|(m, _)| TokenKind::OldFileMode(m))
            .ok_or(ErrorKind::InvalidFileMode)
        } else if let Some(rest) = line.strip_prefix(b"old mode ") {
          parse_int::<u32>(rest, 8)
            .map(|(m, _)| TokenKind::OldFileMode(m))
            .ok_or(ErrorKind::InvalidFileMode)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'r' => {
        if let Some(rest) = line.strip_prefix(b"rename from ") {
          Ok(TokenKind::RenameFrom(rest))
        } else if let Some(rest) = line.strip_prefix(b"rename to ") {
          Ok(TokenKind::RenameTo(rest))
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'c' => {
        if let Some(rest) = line.strip_prefix(b"copy from ") {
          Ok(TokenKind::CopyFrom(rest))
        } else if let Some(rest) = line.strip_prefix(b"copy to ") {
          Ok(TokenKind::CopyTo(rest))
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b's' => {
        if let Some(rest) = line.strip_prefix(b"similarity index ") {
          self.parse_percentage(rest).map(TokenKind::Similarity)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'B' => {
        if let Some(rest) = line.strip_prefix(b"Binary files ") {
          if let Some(line_content) = rest.strip_suffix(b" differ") {
            let mut parts = line_content.split_str(b" and ");
            if let (Some(old_file), Some(new_file)) =
              (parts.next(), parts.next())
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
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      _ => self.parse_non_keyword_line(line),
    };

    Some(
      token_result
        .map(|token| LexerItem { token, line_num })
        .map_err(|kind| Error::with_line(kind, line_num)),
    )
  }

  #[inline]
  fn next_line(&mut self) -> Option<&'a [u8]> {
    if self.pos >= self.input.len() {
      return None;
    }
    self.line_num += 1;
    let remaining = &self.input[self.pos..];
    let end = memchr::memchr(b'\n', remaining).unwrap_or(remaining.len());
    let line = &remaining[..end];
    self.pos += end + 1;

    // Normalize line endings: strip trailing \r
    Some(line.strip_suffix(b"\r").unwrap_or(line))
  }

  fn parse_range(&self, range_bytes: &[u8]) -> Result<(u32, u32), ErrorKind> {
    let (line, rest) = parse_int::<u32>(range_bytes, 10)
      .ok_or(ErrorKind::InvalidHunkRangeLine)?;

    if rest.is_empty() {
      return Ok((line, 1));
    }

    let rest = rest
      .strip_prefix(b",")
      .ok_or(ErrorKind::InvalidHunkRangeLine)?;

    let (span, rest) =
      parse_int::<u32>(rest, 10).ok_or(ErrorKind::InvalidHunkRangeSpan)?;

    if !rest.is_empty() {
      return Err(ErrorKind::InvalidHunkRangeSpan);
    }

    Ok((line, span))
  }

  fn parse_hunk_header(
    &mut self,
    header: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    self.last_line_was_new_file = false;
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

  fn parse_percentage(&self, s: &[u8]) -> Result<u32, ErrorKind> {
    let s = s.strip_suffix(b"%").ok_or(ErrorKind::InvalidPercentage)?;
    let (num, rest) =
      parse_int::<u32>(s, 10).ok_or(ErrorKind::InvalidPercentage)?;
    if !rest.is_empty() {
      return Err(ErrorKind::InvalidPercentage);
    }
    Ok(num)
  }

  fn parse_file_header(
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

  fn parse_index_line(
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

  fn parse_non_keyword_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    match line.first() {
      Some(b'\\') if line == b"\\ No newline at end of file" => {
        if self.last_line_was_new_file {
          Ok(TokenKind::NewFileNoNewline)
        } else {
          Ok(TokenKind::OldFileNoNewline)
        }
      }
      _ => Err(ErrorKind::UnexpectedLine),
    }
  }
}

impl<'a> Iterator for Lexer<'a> {
  type Item = Result<LexerItem<'a>, Error>;

  #[inline]
  fn next(&mut self) -> Option<Self::Item> {
    self.parse_line()
  }
}
