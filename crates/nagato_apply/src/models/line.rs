#[derive(Debug, Clone, PartialEq)]
pub struct Line<'a> {
  pub kind: LineKind,
  pub text: &'a [u8],
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineKind {
  Addition,
  Deletion,
  Context,
}
