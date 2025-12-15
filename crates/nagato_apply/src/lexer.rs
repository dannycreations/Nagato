use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use bstr::ByteSlice;
use memchr::memmem;
use nagato_core::error::{Error, ErrorKind};
use once_cell::sync::Lazy;

use crate::TokenKind;

#[derive(Debug, Clone, PartialEq)]
pub struct LexerItem<'a> {
  pub token: TokenKind<'a>,
  pub line_num: u64,
}

// The Aho-Corasick automaton is used for efficient multi-pattern string matching.
// Building it can be expensive, so we use `once_cell::sync::Lazy` to ensure it's
// constructed only once and shared across all calls, improving performance.
static AUTOMATON: Lazy<AhoCorasick> = Lazy::new(|| {
  // These are the keywords that identify different lines in a git diff.
  // The order is important as it corresponds to the match arms below.
  let patterns = &[
    "diff --git ",
    "index ",
    "--- ",
    "+++ ",
    "@@ ",
    "new file mode ",
    "new mode ",
    "old file mode ",
    "old mode ",
    "deleted file mode ",
    "deleted mode ",
    "rename from ",
    "rename to ",
    "copy from ",
    "copy to ",
    "similarity index ",
    "dissimilarity index ",
    "Binary files ",
  ];

  AhoCorasickBuilder::new()
    .match_kind(MatchKind::LeftmostLongest)
    .build(patterns)
    .unwrap()
});

#[doc(hidden)]
pub struct Lexer<'a> {
  lines: bstr::Lines<'a>,
  line_num: u64,
}

fn strip_git_prefix(s: &[u8]) -> &[u8] {
  // Git diffs often prefix file paths with "a/" or "b/". This function
  // removes them to get the clean file path. It's more efficient than
  // multiple `strip_prefix` calls.
  s.strip_prefix(b"a/")
    .or_else(|| s.strip_prefix(b"b/"))
    .unwrap_or(s)
}

// This is a more efficient implementation that parses a u32 from a byte slice
// without allocating a string. It's used in parsing hunk headers and percentages.
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

impl<'a> Lexer<'a> {
  #[doc(hidden)]
  pub fn new(input: &'a [u8]) -> Self {
    Lexer {
      lines: input.lines(),
      line_num: 0,
    }
  }

  fn parse_line(&mut self) -> Option<Result<LexerItem<'a>, Error>> {
    let line = self.next_line()?;
    let line_num = self.line_num;

    // By having the sub-parsers return `ErrorKind`, we can centralize the creation
    // of the `Error` struct here. This simplifies the sub-parsers and ensures
    // the line number is always correctly associated with the error.
    let token_result: Result<TokenKind, ErrorKind> = if let Some(mat) =
      AUTOMATON.find(line)
    {
      if mat.start() == 0 {
        let rest = line[mat.end()..].trim_end();
        match mat.pattern().as_usize() {
          0 => self.parse_file_header(rest),
          1 => self.parse_index_line(rest),
          2 => Ok(TokenKind::OldFile(strip_git_prefix(rest))),
          3 => Ok(TokenKind::NewFile(strip_git_prefix(rest))),
          4 => self.parse_hunk_header(rest),
          5 | 6 => self.parse_octal_mode(rest).map(TokenKind::NewFileMode),
          7 | 8 => self.parse_octal_mode(rest).map(TokenKind::OldFileMode),
          9 | 10 => self.parse_octal_mode(rest).map(TokenKind::DeletedFileMode),
          11 => Ok(TokenKind::RenameFrom(rest)),
          12 => Ok(TokenKind::RenameTo(rest)),
          13 => Ok(TokenKind::CopyFrom(rest)),
          14 => Ok(TokenKind::CopyTo(rest)),
          15 => self.parse_percentage(rest).map(TokenKind::Similarity),
          16 => self.parse_percentage(rest).map(TokenKind::Dissimilarity),
          17 => {
            if let Some(line_content) = rest.strip_suffix(b" differ") {
              let mut parts = line_content.split_str(b" and ");
              if let (Some(old_file), Some(new_file)) =
                (parts.next(), parts.next())
              {
                Ok(TokenKind::Binary { old_file, new_file })
              } else {
                Err(ErrorKind::InvalidBinaryFilesLine)
              }
            } else {
              Err(ErrorKind::InvalidBinaryFilesLine)
            }
          }
          _ => unreachable!(),
        }
      } else {
        self.parse_non_keyword_line(line)
      }
    } else {
      self.parse_non_keyword_line(line)
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

  // The hunk header parsing is now more efficient by working directly on byte slices,
  // which avoids the overhead of string conversion and validation.
  fn parse_hunk_header(
    &self,
    header: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
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

  fn parse_octal_mode(&self, s: &[u8]) -> Result<u32, ErrorKind> {
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

  fn parse_file_header(
    &self,
    rest: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    // The logic is now more robust, using `strip_git_prefix` to handle file
    // paths that may or may not have the `a/` or `b/` prefixes.
    let mut parts = rest.fields();
    let old_file = parts.next().map(strip_git_prefix);
    let new_file = parts.next().map(strip_git_prefix);

    if let (Some(old_file), Some(new_file)) = (old_file, new_file) {
      Ok(TokenKind::FileHeader { old_file, new_file })
    } else {
      Err(ErrorKind::InvalidFileHeader)
    }
  }

  // This function is optimized to parse the index line from a byte slice. It converts
  // the hash part to a string slice as required by the `Token::Index` struct,
  // but still avoids string allocation for parsing the mode.
  fn parse_index_line(
    &self,
    rest: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    let mut parts = rest.fields();
    let hashes_bytes = parts.next().ok_or(ErrorKind::InvalidIndexLine)?;
    let (old_hash, new_hash) = hashes_bytes
      .split_once_str(b"..")
      .ok_or(ErrorKind::InvalidIndexHashRange)?;
    let mode = parts.next().map(|s| self.parse_octal_mode(s)).transpose()?;
    Ok(TokenKind::Index {
      old_hash,
      new_hash,
      mode,
    })
  }

  fn parse_non_keyword_line(
    &self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    match line.first() {
      Some(b'+') => Ok(TokenKind::Addition(&line[1..])),
      Some(b'-') => Ok(TokenKind::Deletion(&line[1..])),
      Some(b' ') => Ok(TokenKind::Context(&line[1..])),
      Some(b'\\') if line == b"\\ No newline at end of file" => {
        Ok(TokenKind::NoNewline)
      }
      None => Ok(TokenKind::Context(&[])),
      // The fallback logic for parsing a header-less diff is now more memory-efficient.
      // Instead of collecting parts into a `Vec`, it uses an iterator directly,
      // avoiding heap allocation for every non-keyword line.
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
