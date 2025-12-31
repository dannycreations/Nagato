use std::mem;

use crate::{Line, LineKind};

/// Represents a single hunk in a patch.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct Hunk<'a> {
  /// The starting line number of the old file.
  pub old_line: u32,
  /// The number of lines in the old file.
  pub old_span: u32,
  /// The starting line number of the new file.
  pub new_line: u32,
  /// The number of lines in the new file.
  pub new_span: u32,
  /// The lines in the hunk.
  pub lines: Box<[Line<'a>]>,
  /// This will be populated by the parser using the line number from the LexerItem.
  pub patch_line_num: u32,
  /// Whether the hunk has a header.
  pub has_header: bool,
  /// An optional label for matching.
  pub label: Option<&'a [u8]>,
}

impl<'a> Hunk<'a> {
  /// Invert the hunk for reverse application.
  pub fn invert(&mut self) {
    mem::swap(&mut self.old_line, &mut self.new_line);
    mem::swap(&mut self.old_span, &mut self.new_span);
    for line in self.lines.iter_mut() {
      line.kind = match line.kind {
        LineKind::Addition => LineKind::Deletion,
        LineKind::Deletion => LineKind::Addition,
        LineKind::Context => LineKind::Context,
      };
    }
  }
}
