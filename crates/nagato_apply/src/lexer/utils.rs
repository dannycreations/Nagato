#[inline(always)]
pub fn strip_git_prefix(s: &[u8]) -> &[u8] {
  s.strip_prefix(b"a/")
    .or_else(|| s.strip_prefix(b"b/"))
    .unwrap_or(s)
}

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
