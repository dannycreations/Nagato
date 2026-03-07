use std::{
  borrow::Cow,
  io::{Result as IoResult, Write},
  path::PathBuf,
};

use bstr::ByteSlice;

#[inline(always)]
pub fn strip_diff_prefix(s: &[u8]) -> &[u8] {
  if s.len() >= 2 && (s[0] == b'a' || s[0] == b'b') && s[1] == b'/' {
    &s[2..]
  } else {
    s
  }
}

pub fn unquote_path(s: &[u8]) -> Cow<'_, [u8]> {
  if s.len() < 2 || s[0] != b'\"' || s[s.len() - 1] != b'\"' {
    return Cow::Borrowed(strip_diff_prefix(s));
  }

  let s = &s[1..s.len() - 1];
  let first_esc = match s.find_byte(b'\\') {
    Some(idx) => idx,
    None => return Cow::Borrowed(strip_diff_prefix(s)),
  };

  // Pre-stripping the prefix if it exists before the first escape.
  let start_idx =
    if first_esc >= 2 && (s[0] == b'a' || s[0] == b'b') && s[1] == b'/' {
      2
    } else {
      0
    };

  let mut res = Vec::with_capacity(s.len() - start_idx);
  res.extend_from_slice(&s[start_idx..first_esc]);

  let mut i = first_esc;
  while i < s.len() {
    let b = s[i];
    if b == b'\\' && i + 1 < s.len() {
      i += 1;
      let escaped = match s[i] {
        b'n' => b'\n',
        b'r' => b'\r',
        b't' => b'\t',
        b'\\' => b'\\',
        b'\"' => b'\"',
        b @ b'0'..=b'7' => {
          let mut octal = (b - b'0') as u32;
          for _ in 0..2 {
            if let Some(&next_byte) = s.get(i + 1) {
              if (b'0'..=b'7').contains(&next_byte) {
                i += 1;
                octal = (octal << 3) | ((next_byte - b'0') as u32);
                continue;
              }
            }
            break;
          }
          octal as u8
        }
        next => next,
      };
      res.push(escaped);
    } else {
      res.push(b);
    }
    i += 1;
  }

  if start_idx == 0 {
    let stripped = strip_diff_prefix(&res);
    if stripped.len() != res.len() {
      return Cow::Owned(stripped.to_vec());
    }
  }

  Cow::Owned(res)
}

#[allow(clippy::type_complexity)]
pub fn split_diff_paths(line: &[u8]) -> Option<(Cow<'_, [u8]>, Cow<'_, [u8]>)> {
  let line = line.trim();
  if line.is_empty() {
    return None;
  }

  let (p1, rest) = next_path(line)?;
  let (p2, _) = next_path(rest.trim())?;

  Some((unquote_path(p1), unquote_path(p2)))
}

pub fn next_path(s: &[u8]) -> Option<(&[u8], &[u8])> {
  let s = s.trim_start();
  if s.is_empty() {
    return None;
  }
  if s[0] == b'\"' {
    let mut i = 1;
    while i < s.len() {
      let b = s[i];
      if b == b'\"' {
        return Some((&s[..i + 1], s[i + 1..].trim_start()));
      }
      if b == b'\\' {
        i += 1;
      }
      i += 1;
    }
    None
  } else {
    let (path, rest) = match s.find_byte(b' ') {
      Some(idx) => s.split_at(idx),
      None => (s, &[][..]),
    };
    Some((path, rest.trim_start()))
  }
}

#[allow(clippy::type_complexity)]
pub fn next_path_pair<'a>(
  s: &'a [u8],
  separator: &[u8],
) -> Option<(Cow<'a, [u8]>, Cow<'a, [u8]>)> {
  let (p1, rest) = next_path(s)?;
  let rest = if !separator.is_empty() {
    rest.strip_prefix(separator)?.trim()
  } else {
    rest
  };
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

  let (line, rest) = match memchr::memchr(b'\n', source) {
    Some(idx) => (&source[..idx], &source[idx + 1..]),
    None => (source, &[][..]),
  };

  if line.ends_with(b"\r") {
    Some((&line[..line.len() - 1], rest))
  } else {
    Some((line, rest))
  }
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
    if !self.is_first_line {
      self.output.write_all(b"\n")?;
    } else {
      self.is_first_line = false;
    }
    self.output.write_all(line)
  }

  #[inline]
  pub fn ensure_newline(&mut self) -> IoResult<()> {
    if self.is_first_line {
      self.is_first_line = false;
      return Ok(());
    }
    self.output.write_all(b"\n")
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
  pub fn reset_to_first_line(&mut self) {
    self.is_first_line = true;
  }

  #[inline]
  pub fn output(&mut self) -> &mut W {
    self.output
  }
}

#[inline]
pub fn to_path_buf(bytes: &[u8]) -> Result<PathBuf, crate::Error> {
  #[cfg(unix)]
  {
    use std::os::unix::ffi::OsStrExt;
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
  }
  #[cfg(windows)]
  {
    bytes
      .to_str()
      .map(PathBuf::from)
      .map_err(|_| crate::Error::new(crate::ErrorKind::InvalidPath))
  }
}
