use bstr::ByteSlice;

/// Strip common git prefixes "a/" and "b/" from a path.
#[inline(always)]
pub fn strip_git_prefix(s: &[u8]) -> &[u8] {
  s.strip_prefix(b"a/")
    .or_else(|| s.strip_prefix(b"b/"))
    .unwrap_or(s)
}

/// Parse an integer from a byte slice.
#[inline]
pub fn parse_int<T>(bytes: &[u8], radix: u32) -> Option<(T, &[u8])>
where
  T: TryFrom<u64> + Default + Copy,
{
  if bytes.is_empty() {
    return None;
  }
  let mut num: u64 = 0;
  let mut i = 0;
  while i < bytes.len() {
    let b = bytes[i];
    let digit = match b {
      b'0'..=b'9' if radix >= 10 => (b - b'0') as u32,
      b'0'..=b'7' if radix == 8 => (b - b'0') as u32,
      _ => break,
    };
    num = num.checked_mul(radix as u64)?.checked_add(digit as u64)?;
    i += 1;
  }
  if i == 0 {
    return None;
  }
  Some((T::try_from(num).ok()?, &bytes[i..]))
}

/// Get the next line from a source, handling \n and \r\n.
/// Returns (line_content_without_newline, remaining_source).
#[inline(always)]
pub fn get_line(source: &[u8]) -> Option<(&[u8], &[u8])> {
  if source.is_empty() {
    return None;
  }
  let end = source.find_byte(b'\n').unwrap_or(source.len());
  let (full_line, next_source) = if end < source.len() {
    (&source[..end], &source[end + 1..])
  } else {
    (source, &[][..])
  };
  let line_content = full_line.strip_suffix(b"\r").unwrap_or(full_line);
  Some((line_content, next_source))
}
