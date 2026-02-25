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
  if s.len() >= 2 && s[0] == b'"' && s[s.len() - 1] == b'"' {
    let s = &s[1..s.len() - 1];

    if let Some(first_esc) = s.find_byte(b'\\') {
      let mut res = Vec::with_capacity(s.len());
      res.extend_from_slice(&s[..first_esc]);

      let mut i = first_esc;
      while i < s.len() {
        let b = s[i];
        if b == b'\\' && i + 1 < s.len() {
          i += 1;
          match s[i] {
            b'n' => res.push(b'\n'),
            b'r' => res.push(b'\r'),
            b't' => res.push(b'\t'),
            b'\\' => res.push(b'\\'),
            b'"' => res.push(b'"'),
            n @ b'0'..=b'7' => {
              let mut octal = (n - b'0') as u32;
              if i + 1 < s.len() && (b'0'..=b'7').contains(&s[i + 1]) {
                i += 1;
                octal = (octal << 3) | ((s[i] - b'0') as u32);
                if i + 1 < s.len() && (b'0'..=b'7').contains(&s[i + 1]) {
                  i += 1;
                  octal = (octal << 3) | ((s[i] - b'0') as u32);
                }
              }
              res.push(octal as u8);
            }
            next => res.push(next),
          }
        } else {
          res.push(b);
        }
        i += 1;
      }

      let res_final = if res.starts_with(b"a/") || res.starts_with(b"b/") {
        &res[2..]
      } else {
        &res[..]
      };

      return Cow::Owned(res_final.to_vec());
    }
    return Cow::Borrowed(strip_diff_prefix(s));
  }
  Cow::Borrowed(strip_diff_prefix(s))
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
  if s[0] == b'"' {
    let mut i = 1;
    while i < s.len() {
      if s[i] == b'"' {
        return Some((&s[..i + 1], s[i + 1..].trim_start()));
      }
      if s[i] == b'\\' {
        i += 1;
      }
      i += 1;
    }
    None
  } else {
    let (path, rest) = s.split_once_str(b" ").unwrap_or((s, &[]));
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
      b'a'..=b'z' | b'A'..=b'Z' => (b.to_ascii_uppercase() - b'A') as u32 + 10,
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
