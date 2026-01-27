use std::mem;

use crate::{Line, LineKind};

#[derive(Debug, PartialEq, Default, Clone)]
pub struct Hunk<'a> {
  pub old_line: u32,
  pub old_span: u32,
  pub new_line: u32,
  pub new_span: u32,
  pub lines: Box<[Line<'a>]>,
  pub patch_line_num: u32,
  pub has_header: bool,
  pub label: Option<&'a [u8]>,
}

impl<'a> Hunk<'a> {
  pub fn invert(&mut self) {
    mem::swap(&mut self.old_line, &mut self.new_line);
    mem::swap(&mut self.old_span, &mut self.new_span);
    self.lines.iter_mut().for_each(|line| {
      line.kind = match line.kind {
        LineKind::Addition => LineKind::Deletion,
        LineKind::Deletion => LineKind::Addition,
        LineKind::Context => LineKind::Context,
      };
    });
  }

  #[inline]
  pub fn lines_to_match(
    &self,
  ) -> impl Iterator<Item = (usize, &Line<'a>)> + Clone {
    // Non-addition lines are filtered and enumerated to provide the expected context for hunk matching operations.
    self
      .lines
      .iter()
      .enumerate()
      .filter(|(_, l)| !matches!(l.kind, LineKind::Addition))
  }
}
