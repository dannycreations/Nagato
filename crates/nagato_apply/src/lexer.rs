use bstr::ByteSlice;
use memchr::memmem;
use nagato_core::error::{Error, ErrorKind};

use crate::TokenKind;

#[derive(Debug, Clone, PartialEq)]
pub struct LexerItem<'a> {
  pub token: TokenKind<'a>,
  pub line_num: u32,
}

#[doc(hidden)]
pub struct Lexer<'a> {
  lines: bstr::Lines<'a>,
  line_num: u32,
  last_line_was_new_file: bool,
}

fn strip_git_prefix(s: &[u8]) -> &[u8] {
  s.strip_prefix(b"a/")
    .or_else(|| s.strip_prefix(b"b/"))
    .unwrap_or(s)
}

fn parse_u32(bytes: &[u8]) -> Option<(u32, &[u8])> {
  let mut num = 0u32;
  let mut i = 0;
  while i < bytes.len() && bytes[i].is_ascii_digit() {
    num = num
      .checked_mul(10)?
      .checked_add(u32::from(bytes[i] - b'0'))?;
    i += 1;
  }
  if i == 0 {
    None
  } else {
    Some((num, &bytes[i..]))
  }
}

fn parse_octal_mode(s: &[u8]) -> Result<u32, ErrorKind> {
  if s.is_empty() {
    return Err(ErrorKind::InvalidFileMode);
  }
  let mut mode = 0u32;
  for &digit in s {
    if (b'0'..=b'7').contains(&digit) {
      mode = mode
        .checked_mul(8)
        .and_then(|m| m.checked_add(u32::from(digit - b'0')))
        .ok_or(ErrorKind::InvalidFileMode)?;
    } else {
      return Err(ErrorKind::InvalidFileMode);
    }
  }
  Ok(mode)
}

impl<'a> Lexer<'a> {
  #[doc(hidden)]
  pub fn new(input: &'a [u8]) -> Self {
    Lexer {
      lines: input.lines(),
      line_num: 0,
      last_line_was_new_file: false,
    }
  }

  fn parse_line(&mut self) -> Option<Result<LexerItem<'a>, Error>> {
    let line = self.next_line()?;
    let line_num = self.line_num;

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
          parse_octal_mode(rest).map(TokenKind::DeletedFileMode)
        } else if let Some(rest) = line.strip_prefix(b"deleted mode ") {
          parse_octal_mode(rest).map(TokenKind::DeletedFileMode)
        } else if let Some(rest) = line.strip_prefix(b"dissimilarity index ") {
          self.parse_percentage(rest).map(TokenKind::Dissimilarity)
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
          parse_octal_mode(rest).map(TokenKind::NewFileMode)
        } else if let Some(rest) = line.strip_prefix(b"new mode ") {
          parse_octal_mode(rest).map(TokenKind::NewFileMode)
        } else {
          self.parse_non_keyword_line(line)
        }
      }
      b'o' => {
        if let Some(rest) = line.strip_prefix(b"old file mode ") {
          parse_octal_mode(rest).map(TokenKind::OldFileMode)
        } else if let Some(rest) = line.strip_prefix(b"old mode ") {
          parse_octal_mode(rest).map(TokenKind::OldFileMode)
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
        .map_err(|kind| Error {
          line: Some(line_num),
          kind,
        }),
    )
  }

  fn next_line(&mut self) -> Option<&'a [u8]> {
    self.line_num += 1;
    self.lines.next()
  }

  fn parse_range(&self, range_bytes: &[u8]) -> Result<(u32, u32), ErrorKind> {
    let (line, rest) =
      parse_u32(range_bytes).ok_or(ErrorKind::InvalidHunkRangeLine)?;

    if rest.is_empty() {
      return Ok((line, 1));
    }

    let rest = rest
      .strip_prefix(b",")
      .ok_or(ErrorKind::InvalidHunkRangeLine)?;

    let (span, rest) =
      parse_u32(rest).ok_or(ErrorKind::InvalidHunkRangeSpan)?;

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
    let (num, rest) = parse_u32(s).ok_or(ErrorKind::InvalidPercentage)?;
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
    let mode = parts.next().map(parse_octal_mode).transpose()?;
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
      _ => {
        let mut parts = line.fields();
        match (parts.next(), parts.next(), parts.next()) {
          (Some(part1), None, _) => {
            let old_file = strip_git_prefix(part1);
            Ok(TokenKind::FileHeader {
              old_file,
              new_file: old_file,
            })
          }
          (Some(part1), Some(part2), None) => {
            let old_file = strip_git_prefix(part1);
            let new_file = strip_git_prefix(part2);
            Ok(TokenKind::FileHeader { old_file, new_file })
          }
          _ => Err(ErrorKind::UnexpectedLine),
        }
      }
    }
  }
}

impl<'a> Iterator for Lexer<'a> {
  type Item = Result<LexerItem<'a>, Error>;

  fn next(&mut self) -> Option<Self::Item> {
    self.parse_line()
  }
}
