use std::io::{Result as IoResult, Write};

use bstr::ByteSlice;

/// Strip common git prefixes "a/" and "b/" from a path.
#[inline(always)]
pub fn strip_git_prefix(s: &[u8]) -> &[u8] {
  s.strip_prefix(b"a/")
    .or_else(|| s.strip_prefix(b"b/"))
    .unwrap_or(s)
}

/// Parse an integer from a byte slice.
/// Returns the parsed value and the remaining byte slice.
#[inline]
pub fn parse_int<T>(bytes: &[u8], radix: u32) -> Option<(T, &[u8])>
where
  T: TryFrom<u64>,
{
  let mut num = 0u64;
  let mut len = 0;

  for &b in bytes {
    let digit = match b {
      b'0'..=b'9' => (b - b'0') as u32,
      b'a'..=b'z' => (b - b'a') as u32 + 10,
      b'A'..=b'Z' => (b - b'A') as u32 + 10,
      _ => break,
    };

    if digit >= radix {
      break;
    }

    num = num.checked_mul(radix as u64)?.checked_add(digit as u64)?;
    len += 1;
  }

  if len == 0 {
    None
  } else {
    Some((T::try_from(num).ok()?, &bytes[len..]))
  }
}

/// Get the next line from a source, handling \n and \r\n.
/// Returns (line_content_without_newline, remaining_source).
#[inline(always)]
pub fn get_line(source: &[u8]) -> Option<(&[u8], &[u8])> {
  if source.is_empty() {
    return None;
  }

  let (line, rest) = source.split_once_str(b"\n").unwrap_or((source, &[]));
  Some((line.strip_suffix(b"\r").unwrap_or(line), rest))
}

/// Helper to handle line-based writing with automatic newline insertion.
/// Ensures we only insert newlines between lines, not before the first or after the last.
pub struct LineWriter<'a, W: Write + ?Sized> {
  output: &'a mut W,
  /// Tracks if we've written anything yet to manage inter-line newlines.
  is_first_line: bool,
}

impl<'a, W: Write + ?Sized> LineWriter<'a, W> {
  #[inline]
  pub fn new(output: &'a mut W) -> Self {
    Self {
      output,
      is_first_line: true,
    }
  }

  /// Write a line to the output. Prepends a newline if it's not the first line.
  #[inline]
  pub fn write_line(&mut self, line: &[u8]) -> IoResult<()> {
    self.ensure_newline()?;
    self.output.write_all(line)
  }

  /// Ensure a newline is written before the next content, unless it's the first line.
  #[inline]
  pub fn ensure_newline(&mut self) -> IoResult<()> {
    if !self.is_first_line {
      self.output.write_all(b"\n")?;
    } else {
      self.is_first_line = false;
    }
    Ok(())
  }

  /// Write raw bytes to the underlying output.
  #[inline]
  pub fn write_bytes(&mut self, bytes: &[u8]) -> IoResult<()> {
    if !bytes.is_empty() {
      self.is_first_line = false;
    }
    self.output.write_all(bytes)
  }

  /// Write a raw newline character.
  #[inline]
  pub fn write_newline(&mut self) -> IoResult<()> {
    self.is_first_line = false;
    self.output.write_all(b"\n")
  }

  /// Check if no lines have been written yet.
  #[inline]
  pub fn is_first_line(&self) -> bool {
    self.is_first_line
  }

  /// Access the underlying output writer.
  #[inline]
  pub fn output(&mut self) -> &mut W {
    self.output
  }
}
