use std::str;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use bstr::{ByteSlice, B};
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

  fn with_rest_as_str<F, T>(
    &self,
    rest: &'a [u8],
    line: &'a [u8],
    f: F,
  ) -> Result<T, ParseError>
  where
    F: FnOnce(&'a str) -> Result<T, ParseError>,
  {
    // This is a helper to safely convert a byte slice to a UTF-8 string slice.
    // It provides a consistent error handling mechanism for parsing operations
    // that expect string input.
    str::from_utf8(rest)
      .map_err(|_| {
        ParseError::UnexpectedLine(String::from_utf8_lossy(line).into())
      })
      .and_then(f)
  }

  fn parse_range(&self, range_str: &str) -> Result<(u32, u32), ParseError> {
    let (line_str, span_str) =
      range_str.split_once(',').unwrap_or((range_str, "1"));
    let line = line_str
      .parse()
      .map_err(|_| ParseError::InvalidHunkRangeLine(range_str.to_string()))?;
    let span = span_str
      .parse()
      .map_err(|_| ParseError::InvalidHunkRangeSpan(span_str.to_string()))?;
    Ok((line, span))
  }

  fn parse_hunk_header(
    &self,
    header: &'a str,
  ) -> Result<Token<'a>, ParseError> {
    let content = header
      .split(" @@")
      .next()
      .ok_or(ParseError::MalformedHunkHeader)?;
    let mut parts = content.split_whitespace();
    let old_range_str = parts
      .next()
      .and_then(|s| s.strip_prefix('-'))
      .ok_or(ParseError::MissingOldRange)?;
    let new_range_str = parts
      .next()
      .and_then(|s| s.strip_prefix('+'))
      .ok_or(ParseError::MissingNewRange)?;

    let (old_line, old_span) = self.parse_range(old_range_str)?;
    let (new_line, new_span) = self.parse_range(new_range_str)?;

    Ok(Token::HunkHeader {
      old_line,
      old_span,
      new_line,
      new_span,
    })
  }

  fn parse_percentage(&self, s: &str) -> Result<u32, ParseError> {
    s.strip_suffix('%')
      .ok_or_else(|| ParseError::InvalidPercentage(s.to_string()))?
      .parse()
      .map_err(|_| ParseError::InvalidPercentage(s.to_string()))
  }

  fn parse_octal_mode(&self, s: &str) -> Result<u32, ParseError> {
    u32::from_str_radix(s, 8)
      .map_err(|_| ParseError::InvalidFileMode(s.to_string()))
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

  fn parse_index_line(&self, rest: &'a str) -> Result<Token<'a>, ParseError> {
    let mut parts = rest.split_whitespace();
    let hashes = parts.next().ok_or(ParseError::InvalidIndexLine)?;
    let (old_hash, new_hash) = hashes
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
          1 => self.with_rest_as_str(rest, line, |s| self.parse_index_line(s)),
          2 => Ok(Token::OldFile(strip_git_prefix(rest))),
          3 => Ok(Token::NewFile(strip_git_prefix(rest))),
          4 => self.with_rest_as_str(rest, line, |s| self.parse_hunk_header(s)),
          5 | 6 => self.with_rest_as_str(rest, line, |s| {
            self.parse_octal_mode(s).map(Token::NewFileMode)
          }),
          7 | 8 => self.with_rest_as_str(rest, line, |s| {
            self.parse_octal_mode(s).map(Token::OldFileMode)
          }),
          9 | 10 => self.with_rest_as_str(rest, line, |s| {
            self.parse_octal_mode(s).map(Token::DeletedFileMode)
          }),
          11 => Ok(Token::RenameFrom(rest)),
          12 => Ok(Token::RenameTo(rest)),
          13 => Ok(Token::CopyFrom(rest)),
          14 => Ok(Token::CopyTo(rest)),
          15 => self.with_rest_as_str(rest, line, |s| {
            self.parse_percentage(s).map(Token::Similarity)
          }),
          16 => self.with_rest_as_str(rest, line, |s| {
            self.parse_percentage(s).map(Token::Dissimilarity)
          }),
          17 => {
            // This refactoring improves clarity by replacing a dense `and_then` chain
            // with a more readable `if let` structure. It makes the parsing steps explicit.
            if let Some(line_content) = rest.strip_suffix(b" differ") {
              let mut parts = line_content.split_str(B(" and "));
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
