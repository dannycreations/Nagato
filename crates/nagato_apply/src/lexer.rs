use std::str;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use bstr::ByteSlice;
use memchr::memmem;
use nagato_core::error::ParseError;
use once_cell::sync::Lazy;

use crate::Token;

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
    }
  }

  fn next_line(&mut self) -> Option<&'a [u8]> {
    self.lines.next()
  }

  // This function now parses a range from a byte slice, avoiding string conversion
  // for better performance.
  fn parse_range(&self, range_bytes: &[u8]) -> Result<(u32, u32), ParseError> {
    let (line, rest) = parse_u32(range_bytes).ok_or_else(|| {
      ParseError::InvalidHunkRangeLine(
        String::from_utf8_lossy(range_bytes).into_owned(),
      )
    })?;

    if rest.is_empty() {
      return Ok((line, 1));
    }

    if !rest.starts_with(b",") {
      return Err(ParseError::InvalidHunkRangeLine(
        String::from_utf8_lossy(range_bytes).into_owned(),
      ));
    }

    let (span, rest) = parse_u32(&rest[1..]).ok_or_else(|| {
      ParseError::InvalidHunkRangeSpan(
        String::from_utf8_lossy(range_bytes).into_owned(),
      )
    })?;

    if !rest.is_empty() {
      return Err(ParseError::InvalidHunkRangeSpan(
        String::from_utf8_lossy(range_bytes).into_owned(),
      ));
    }

    Ok((line, span))
  }

  // The hunk header parsing is now more efficient by working directly on byte slices,
  // which avoids the overhead of string conversion and validation.
  fn parse_hunk_header(
    &self,
    header: &'a [u8],
  ) -> Result<Token<'a>, ParseError> {
    let content_end = memmem::find(header, b" @@").unwrap_or(header.len());
    let content = &header[..content_end];
    let mut parts = content.fields();

    let old_range_bytes = parts
      .next()
      .and_then(|s: &[u8]| s.strip_prefix(b"-"))
      .ok_or(ParseError::MissingOldRange)?;
    let new_range_bytes = parts
      .next()
      .and_then(|s: &[u8]| s.strip_prefix(b"+"))
      .ok_or(ParseError::MissingNewRange)?;

    let (old_line, old_span) = self.parse_range(old_range_bytes)?;
    let (new_line, new_span) = self.parse_range(new_range_bytes)?;

    Ok(Token::HunkHeader {
      old_line,
      old_span,
      new_line,
      new_span,
    })
  }

  // This function now parses a percentage from a byte slice, avoiding string conversion
  // for better performance.
  fn parse_percentage(&self, s: &[u8]) -> Result<u32, ParseError> {
    let s = s.strip_suffix(b"%").ok_or_else(|| {
      ParseError::InvalidPercentage(String::from_utf8_lossy(s).into_owned())
    })?;
    let (num, rest) = parse_u32(s).ok_or_else(|| {
      ParseError::InvalidPercentage(String::from_utf8_lossy(s).into_owned())
    })?;
    if !rest.is_empty() {
      return Err(ParseError::InvalidPercentage(
        String::from_utf8_lossy(s).into_owned(),
      ));
    }
    Ok(num)
  }

  // This function now parses an octal mode from a byte slice, which is faster
  // than converting to a string first.
  fn parse_octal_mode(&self, s: &[u8]) -> Result<u32, ParseError> {
    let mut mode = 0u32;
    for &digit in s {
      if (b'0'..=b'7').contains(&digit) {
        mode = mode
          .checked_mul(8)
          .and_then(|m| m.checked_add(u32::from(digit - b'0')))
          .ok_or_else(|| {
            ParseError::InvalidFileMode(String::from_utf8_lossy(s).into_owned())
          })?;
      } else {
        return Err(ParseError::InvalidFileMode(
          String::from_utf8_lossy(s).into_owned(),
        ));
      }
    }
    Ok(mode)
  }

  fn parse_file_header(&self, rest: &'a [u8]) -> Result<Token<'a>, ParseError> {
    // Using if-let and explicit returns improves readability over chained `and_then` calls,
    // making the parsing logic easier to follow without a performance penalty.
    let mut parts = rest.fields();
    if let (Some(old_file_part), Some(new_file_part)) =
      (parts.next(), parts.next())
    {
      if let (Some(old_file), Some(new_file)) = (
        old_file_part.strip_prefix(b"a/"),
        new_file_part.strip_prefix(b"b/"),
      ) {
        return Ok(Token::FileHeader { old_file, new_file });
      }
    }
    Err(ParseError::InvalidFileHeader)
  }

  // This function is optimized to parse the index line from a byte slice. It converts
  // the hash part to a string slice as required by the `Token::Index` struct,
  // but still avoids string allocation for parsing the mode.
  fn parse_index_line(&self, rest: &'a [u8]) -> Result<Token<'a>, ParseError> {
    let mut parts = rest.fields();
    let hashes_bytes = parts.next().ok_or(ParseError::InvalidIndexLine)?;
    let hashes_str =
      str::from_utf8(hashes_bytes).map_err(|_| ParseError::InvalidIndexLine)?;
    let (old_hash, new_hash) = hashes_str
      .split_once("..")
      .ok_or(ParseError::InvalidIndexHashRange)?;
    let mode = parts.next().map(|s| self.parse_octal_mode(s)).transpose()?;
    Ok(Token::Index {
      old_hash,
      new_hash,
      mode,
    })
  }

  fn parse_line(&mut self) -> Option<Result<Token<'a>, ParseError>> {
    let line = self.next_line()?;

    // Using the Aho-Corasick automaton is much faster for matching multiple
    // keywords than iterating through a list of prefixes and calling `starts_with`
    // for each one.
    if let Some(mat) = AUTOMATON.find(line) {
      if mat.start() == 0 {
        let rest = line[mat.end()..].trim_end();
        // Directly matching on the pattern index is more efficient than using a
        // separate `Keyword` enum and a `Vec` lookup. This reduces memory usage
        // and simplifies the code.
        return Some(match mat.pattern().as_usize() {
          0 => self.parse_file_header(rest),
          1 => self.parse_index_line(rest),
          2 => Ok(Token::OldFile(strip_git_prefix(rest))),
          3 => Ok(Token::NewFile(strip_git_prefix(rest))),
          4 => self.parse_hunk_header(rest),
          5 | 6 => self.parse_octal_mode(rest).map(Token::NewFileMode),
          7 | 8 => self.parse_octal_mode(rest).map(Token::OldFileMode),
          9 | 10 => self.parse_octal_mode(rest).map(Token::DeletedFileMode),
          11 => Ok(Token::RenameFrom(rest)),
          12 => Ok(Token::RenameTo(rest)),
          13 => Ok(Token::CopyFrom(rest)),
          14 => Ok(Token::CopyTo(rest)),
          15 => self.parse_percentage(rest).map(Token::Similarity),
          16 => self.parse_percentage(rest).map(Token::Dissimilarity),
          17 => {
            // This refactoring improves clarity by replacing a dense `and_then` chain
            // with a more readable `if let` structure. It makes the parsing steps explicit.
            if let Some(line_content) = rest.strip_suffix(b" differ") {
              let mut parts = line_content.split_str(b" and ");
              if let (Some(old_file), Some(new_file)) =
                (parts.next(), parts.next())
              {
                Ok(Token::Binary { old_file, new_file })
              } else {
                Err(ParseError::InvalidBinaryFilesLine)
              }
            } else {
              Err(ParseError::InvalidBinaryFilesLine)
            }
          }
          _ => unreachable!(),
        });
      }
    }

    match line.first() {
      Some(b'+') => return Some(Ok(Token::Addition(&line[1..]))),
      Some(b'-') => return Some(Ok(Token::Deletion(&line[1..]))),
      Some(b' ') => return Some(Ok(Token::Context(&line[1..]))),
      Some(b'\\') if line == b"\\ No newline at end of file" => {
        return Some(Ok(Token::NoNewline))
      }
      None => return Some(Ok(Token::Context(&[]))),
      _ => {}
    }

    // This is a fallback for headerless diffs. It's less common but
    // needs to be supported for broader compatibility.
    let mut parts = line.fields();
    if let Some(first) = parts.next() {
      let old_file = strip_git_prefix(first);
      let new_file = parts.next().map(strip_git_prefix).unwrap_or(old_file);
      if parts.next().is_none() {
        return Some(Ok(Token::FileHeader { old_file, new_file }));
      }
    }

    Some(Err(ParseError::UnexpectedLine(
      String::from_utf8_lossy(line).to_string(),
    )))
  }
}

impl<'a> Iterator for Lexer<'a> {
  type Item = Result<Token<'a>, ParseError>;

  fn next(&mut self) -> Option<Self::Item> {
    self.parse_line()
  }
}
