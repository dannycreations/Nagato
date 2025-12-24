use bstr::ByteSlice;

#[inline(always)]
pub fn get_line(source: &[u8]) -> Option<(&[u8], &[u8])> {
  if source.is_empty() {
    return None;
  }
  let end = source.find_byte(b'\n').unwrap_or(source.len());
  let full_line = &source[..end];
  let next_source = if end < source.len() {
    &source[end + 1..]
  } else {
    &[]
  };
  let line_content = full_line.strip_suffix(b"\r").unwrap_or(full_line);
  Some((line_content, next_source))
}
