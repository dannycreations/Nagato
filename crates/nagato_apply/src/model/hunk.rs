use std::mem;

use crate::{Line, LineKind};

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

const _: () = assert!(std::mem::size_of::<Hunk>() == 48);

impl<'a> Hunk<'a> {
  pub fn invert(&mut self) {
    mem::swap(&mut self.old_line, &mut self.new_line);
    mem::swap(&mut self.old_span, &mut self.new_span);
  }

  #[inline]
  pub fn lines_to_match<'h>(
    &self,
    lines: &'h [Line<'a>],
  ) -> impl Iterator<Item = (usize, &'h Line<'a>)> + Clone {
    lines
      .iter()
      .enumerate()
      .filter(|(_, l)| !matches!(l.kind, LineKind::Addition))
  }

  #[inline]
  pub fn first_non_empty_match_line<'h>(
    &self,
    lines: &'h [Line<'a>],
  ) -> Option<(usize, &'h Line<'a>)> {
    self.lines_to_match(lines).find(|(_, l)| !l.text.is_empty())
  }

  #[inline]
  pub fn best_match_line<'h>(
    &self,
    lines: &'h [Line<'a>],
  ) -> Option<(usize, &'h Line<'a>)> {
    self
      .lines_to_match(lines)
      .filter(|(_, l)| !l.text.is_empty())
      .max_by_key(|(_, l)| l.text.len())
  }
}
