#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum LineKind {
  Addition,
  Deletion,
  Context,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line<'a> {
  pub kind: LineKind,
  pub text: &'a [u8],
}
