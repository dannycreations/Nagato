use std::io::{Result as IoResult, Write};

use bstr::ByteSlice;

#[inline(always)]
pub fn strip_git_prefix(s: &[u8]) -> &[u8] {
  s.strip_prefix(b"a/")
    .or_else(|| s.strip_prefix(b"b/"))
    .unwrap_or(s)
}

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

  if len > 0 {
    Some((T::try_from(num).ok()?, &bytes[len..]))
  } else {
    None
  }
}

#[inline(always)]
pub fn get_line(source: &[u8]) -> Option<(&[u8], &[u8])> {
  if source.is_empty() {
    return None;
  }

  let (line, rest) = source.split_once_str(b"\n").unwrap_or((source, &[]));
  Some((line.strip_suffix(b"\r").unwrap_or(line), rest))
}

pub struct LineWriter<'a, W: Write + ?Sized> {
  output: &'a mut W,
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

  #[inline]
  pub fn write_line(&mut self, line: &[u8]) -> IoResult<()> {
    self.ensure_newline()?;
    self.output.write_all(line)
  }

  #[inline]
  pub fn ensure_newline(&mut self) -> IoResult<()> {
    if !self.is_first_line {
      self.output.write_all(b"\n")?;
    } else {
      self.is_first_line = false;
    }
    Ok(())
  }

  #[inline]
  pub fn write_bytes(&mut self, bytes: &[u8]) -> IoResult<()> {
    if !bytes.is_empty() {
      self.is_first_line = false;
    }
    self.output.write_all(bytes)
  }

  #[inline]
  pub fn write_newline(&mut self) -> IoResult<()> {
    self.is_first_line = false;
    self.output.write_all(b"\n")
  }

  #[inline]
  pub fn is_first_line(&self) -> bool {
    self.is_first_line
  }

  #[inline]
  pub fn output(&mut self) -> &mut W {
    self.output
  }
}
