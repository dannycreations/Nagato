use std::mem;

#[derive(Debug, PartialEq, Default, Clone)]
pub struct Hunk<'a> {
  pub old_line: u32,
  pub old_span: u32,
  pub new_line: u32,
  pub new_span: u32,
  pub lines_start: u32,
  pub lines_len: u32,
  pub patch_line_num: u32,
  pub has_header: bool,
  pub label: Option<&'a [u8]>,
}

impl Hunk<'_> {
  pub fn invert(&mut self) {
    mem::swap(&mut self.old_line, &mut self.new_line);
    mem::swap(&mut self.old_span, &mut self.new_span);
  }
}
