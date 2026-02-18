use std::{
  borrow::Cow,
  io::{Result as IoResult, Write},
  path::PathBuf,
};

use bstr::ByteSlice;

#[inline(always)]
pub fn strip_diff_prefix(s: &[u8]) -> &[u8] {
  s.strip_prefix(b"a/")
    .or_else(|| s.strip_prefix(b"b/"))
    .unwrap_or(s)
}

pub fn unquote_path(s: &[u8]) -> Cow<'_, [u8]> {
  // Path unquoting logic uses an early-exit strategy for unquoted and non-escaped paths to minimize allocations and redundant prefix checks.
  if s.len() < 2 || s[0] != b'"' || s[s.len() - 1] != b'"' {
    return Cow::Borrowed(strip_diff_prefix(s));
  }

  let content = &s[1..s.len() - 1];
  if !content.contains(&b'\\') {
    return Cow::Borrowed(strip_diff_prefix(content));
  }

  let mut res = Vec::with_capacity(content.len());
  let mut it = content.iter().peekable();
  while let Some(&b) = it.next() {
    if b == b'\\' {
      match it.next() {
        Some(b'n') => res.push(b'\n'),
        Some(b'r') => res.push(b'\r'),
        Some(b't') => res.push(b'\t'),
        Some(b'\\') => res.push(b'\\'),
        Some(b'"') => res.push(b'"'),
        Some(n @ b'0'..=b'7') => {
          let mut octal = (n - b'0') as u32;
          for _ in 0..2 {
            if let Some(&&n) =
              it.peek().filter(|&&&n| (b'0'..=b'7').contains(&n))
            {
              octal = (octal << 3) | ((n - b'0') as u32);
              it.next();
            } else {
              break;
            }
          }
          res.push(octal as u8);
        }
        Some(&next) => res.push(next),
        None => break,
      }
    } else {
      res.push(b);
    }
  }

  match strip_diff_prefix(&res) {
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

  let (p1, rest) = next_path(line)?;
  let (p2, _) = next_path(rest.trim())?;

  Some((unquote_path(p1), unquote_path(p2)))
}

pub fn next_path(s: &[u8]) -> Option<(&[u8], &[u8])> {
  if s.is_empty() {
    return None;
  }
  if s[0] == b'"' {
    let mut it = s.iter().enumerate().skip(1);
    while let Some((i, &b)) = it.next() {
      if b == b'"' {
        return Some((&s[..i + 1], s[i + 1..].trim()));
      }
      if b == b'\\' {
        it.next();
      }
    }
    None
  } else {
    let (path, rest) = s.split_once_str(b" ").unwrap_or((s, &[]));
    Some((path, rest.trim()))
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
  let mut it = bytes.iter().peekable();
  let mut len = 0;

  while let Some(&&b) = it.peek() {
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
    it.next();
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
    self.write_bytes(line)
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
