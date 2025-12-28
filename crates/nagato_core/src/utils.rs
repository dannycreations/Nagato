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
  let mut len = 0;
  let mut num = 0u64;

  for &b in bytes {
    let digit = if b.is_ascii_digit() {
      (b - b'0') as u32
    } else if b.is_ascii_lowercase() {
      (b - b'a') as u32 + 10
    } else if b.is_ascii_uppercase() {
      (b - b'A') as u32 + 10
    } else {
      break;
    };

    if digit >= radix {
      break;
    }

    num = num.checked_mul(radix as u64)?.checked_add(digit as u64)?;
    len += 1;
  }

  if len == 0 {
    return None;
  }

  Some((T::try_from(num).ok()?, &bytes[len..]))
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
