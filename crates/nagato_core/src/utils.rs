#[cfg(unix)]
use std::ffi::OsStr;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::{
  borrow::Cow,
  io::{Result as IoResult, Write},
  path::PathBuf,
};

use bstr::ByteSlice;
use memchr::{memchr, memchr2};

use crate::Error;
#[cfg(windows)]
use crate::ErrorKind;

#[inline(always)]
pub fn strip_diff_prefix(s: &[u8]) -> &[u8] {
  if s.starts_with(b"a/") || s.starts_with(b"b/") {
    return &s[2..];
  }
  s
}

pub fn unquote_path(s: &[u8]) -> Cow<'_, [u8]> {
  if s.len() < 2 || s[0] != b'\"' || s[s.len() - 1] != b'\"' {
    return Cow::Borrowed(strip_diff_prefix(s));
  }

  let inner = &s[1..s.len() - 1];
  let Some(first_esc) = memchr(b'\\', inner) else {
    return Cow::Borrowed(strip_diff_prefix(inner));
  };

  let mut res = Vec::with_capacity(inner.len());
  res.extend_from_slice(&inner[..first_esc]);

  let mut i = first_esc;
  while i < inner.len() {
    let b = inner[i];
    if b != b'\\' {
      res.push(b);
      i += 1;
      continue;
    }

    if i + 1 >= inner.len() {
      res.push(b);
      i += 1;
      continue;
    }

    i += 1;
    let escaped = match inner[i] {
      b'n' => b'\n',
      b'r' => b'\r',
      b't' => b'\t',
      b'\\' => b'\\',
      b'\"' => b'\"',
      b @ b'0'..=b'7' => decode_octal(inner, &mut i, b),
      next => next,
    };
    res.push(escaped);
    i += 1;
  }

  if res.starts_with(b"a/") || res.starts_with(b"b/") {
    res.drain(..2);
  }

  Cow::Owned(res)
}

pub fn next_path(s: &[u8]) -> Option<(&[u8], &[u8])> {
  let s = s.trim_start();
  if s.is_empty() {
    return None;
  }
  if s[0] != b'\"' {
    let Some(idx) = memchr(b' ', s) else {
      return Some((s, &[][..]));
    };

    let (path, rest) = s.split_at(idx);
    return Some((path, rest.trim_start()));
  }

  let mut i = 1;
  while i < s.len() {
    let idx = memchr2(b'\"', b'\\', &s[i..])?;
    i += idx;
    if s[i] == b'\"' {
      return Some((&s[..i + 1], s[i + 1..].trim_start()));
    }
    i += 2;
  }
  None
}

#[allow(clippy::type_complexity)]
pub fn next_path_pair<'a>(
  s: &'a [u8],
  separator: &[u8],
) -> Option<(Cow<'a, [u8]>, Cow<'a, [u8]>)> {
  let (p1, rest) = next_path(s)?;
  let mut rest = rest;
  if !separator.is_empty() {
    rest = rest.strip_prefix(separator)?.trim();
  }
  let (p2, _) = next_path(rest)?;

  Some((unquote_path(p1), unquote_path(p2)))
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

    num = num
      .checked_mul(u64::from(radix))?
      .checked_add(u64::from(digit))?;
    len += 1;
  }

  if len == 0 {
    return None;
  }

  let val = T::try_from(num).ok()?;
  Some((val, &bytes[len..]))
}

#[inline(always)]
pub fn get_line(source: &[u8]) -> Option<(&[u8], &[u8])> {
  if source.is_empty() {
    return None;
  }

  let Some(idx) = memchr(b'\n', source) else {
    let mut line = source;
    if let Some((&b'\r', _)) = line.split_last() {
      line = &line[..line.len() - 1];
    }
    return Some((line, &[][..]));
  };

  let mut line = &source[..idx];
  let rest = &source[idx + 1..];

  if let Some((&b'\r', _)) = line.split_last() {
    line = &line[..line.len() - 1];
  }

  Some((line, rest))
}

pub struct LineWriter<'a, W: Write + ?Sized> {
  output: &'a mut W,
  last_was_newline: bool,
  is_empty: bool,
}

impl<'a, W: Write + ?Sized> LineWriter<'a, W> {
  #[inline]
  pub fn new(output: &'a mut W) -> Self {
    Self {
      output,
      last_was_newline: false,
      is_empty: true,
    }
  }

  #[inline]
  pub fn write_line(&mut self, line: &[u8]) -> IoResult<()> {
    if !self.is_empty && !self.last_was_newline {
      self.output.write_all(b"\n")?;
    }
    self.is_empty = false;
    self.output.write_all(line)?;
    self.last_was_newline = false;
    Ok(())
  }

  #[inline]
  pub fn ensure_newline(&mut self) -> IoResult<()> {
    if self.is_empty || self.last_was_newline {
      return Ok(());
    }
    self.output.write_all(b"\n")?;
    self.last_was_newline = true;
    Ok(())
  }

  #[inline]
  pub fn write_bytes(&mut self, bytes: &[u8]) -> IoResult<()> {
    if bytes.is_empty() {
      return Ok(());
    }
    self.is_empty = false;
    self.output.write_all(bytes)?;
    self.last_was_newline = bytes.last() == Some(&b'\n');
    Ok(())
  }

  #[inline]
  pub fn write_block(&mut self, block: &[u8]) -> IoResult<()> {
    if block.is_empty() {
      return Ok(());
    }

    if !self.is_empty && !self.last_was_newline {
      self.output.write_all(b"\n")?;
    }

    self.is_empty = false;
    self.output.write_all(block)?;
    self.last_was_newline = block.last() == Some(&b'\n');
    Ok(())
  }

  #[inline]
  pub fn write_newline(&mut self) -> IoResult<()> {
    self.is_empty = false;
    self.output.write_all(b"\n")?;
    self.last_was_newline = true;
    Ok(())
  }

  #[inline]
  pub fn is_first_line(&self) -> bool {
    self.is_empty
  }

  #[inline]
  pub fn output(&mut self) -> &mut W {
    self.output
  }
}

#[inline]
pub fn to_path_buf(bytes: &[u8]) -> Result<PathBuf, Error> {
  #[cfg(unix)]
  {
    Ok(PathBuf::from(OsStr::from_bytes(bytes)))
  }
  #[cfg(windows)]
  {
    bytes
      .to_str()
      .map(PathBuf::from)
      .map_err(|_| Error::new(ErrorKind::InvalidPath))
  }
}

fn decode_octal(inner: &[u8], i: &mut usize, first_digit: u8) -> u8 {
  let mut octal = (first_digit - b'0') as u32;

  let Some(&n1) = inner.get(*i + 1).filter(|&&b| (b'0'..=b'7').contains(&b))
  else {
    return octal as u8;
  };

  *i += 1;
  octal = (octal << 3) | ((n1 - b'0') as u32);

  let Some(&n2) = inner.get(*i + 1).filter(|&&b| (b'0'..=b'7').contains(&b))
  else {
    return octal as u8;
  };

  *i += 1;
  octal = (octal << 3) | ((n2 - b'0') as u32);

  octal as u8
}
