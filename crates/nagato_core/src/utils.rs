use std::{
  borrow::Cow,
  io::{Result as IoResult, Write},
};

use bstr::ByteSlice;

#[inline(always)]
pub fn strip_prefix(s: &[u8]) -> &[u8] {
  s.strip_prefix(b"a/")
    .or_else(|| s.strip_prefix(b"b/"))
    .unwrap_or(s)
}

pub fn unquote_path(s: &[u8]) -> Cow<'_, [u8]> {
  if s.len() < 2 || s[0] != b'"' || s[s.len() - 1] != b'"' {
    return Cow::Borrowed(strip_prefix(s));
  }

  let mut res = Vec::with_capacity(s.len() - 2);
  let mut i = 1;
  while i < s.len() - 1 {
    if s[i] == b'\\' && i + 1 < s.len() - 1 {
      i += 1;
      match s[i] {
        b'"' => res.push(b'"'),
        b'\\' => res.push(b'\\'),
        b't' => res.push(b'\t'),
        b'n' => res.push(b'\n'),
        b'r' => res.push(b'\r'),
        b'0'..=b'7' => {
          let mut octal = s[i] - b'0';
          let mut count = 1;
          while count < 3 && i + 1 < s.len() - 1 {
            let next = s[i + 1];
            if (b'0'..=b'7').contains(&next) {
              octal = octal * 8 + (next - b'0');
              i += 1;
              count += 1;
            } else {
              break;
            }
          }
          res.push(octal);
        }
        _ => res.push(s[i]),
      }
    } else {
      res.push(s[i]);
    }
    i += 1;
  }

  match strip_prefix(&res) {
    s if s.len() == res.len() => Cow::Owned(res),
    s => {
      let start = res.len() - s.len();
      let mut v = res;
      v.drain(0..start);
      Cow::Owned(v)
    }
  }
}

#[allow(clippy::type_complexity)]
pub fn split_diff_paths(line: &[u8]) -> Option<(Cow<'_, [u8]>, Cow<'_, [u8]>)> {
  let line = line.trim();
  if line.is_empty() {
    return None;
  }

  fn next_path(s: &[u8]) -> Option<(&[u8], &[u8])> {
    if s.is_empty() {
      return None;
    }
    if s[0] == b'"' {
      let mut i = 1;
      while i < s.len() {
        if s[i] == b'"' {
          return Some((&s[..i + 1], s[i + 1..].trim()));
        }
        if s[i] == b'\\' && i + 1 < s.len() {
          i += 1;
        }
        i += 1;
      }
      None
    } else {
      let (path, rest) = s.split_once_str(b" ").unwrap_or((s, &[]));
      Some((path, rest.trim()))
    }
  }

  let (p1, rest) = next_path(line)?;
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
