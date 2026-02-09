#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum LineKind {
  Addition,
  Deletion,
  Context,
}

impl LineKind {
  #[inline]
  pub fn invert(&mut self) {
    *self = match self {
      Self::Addition => Self::Deletion,
      Self::Deletion => Self::Addition,
      Self::Context => Self::Context,
    };
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line<'a> {
  pub kind: LineKind,
  pub text: &'a [u8],
}

impl<'a> Line<'a> {
  #[inline]
  pub fn invert(&mut self) {
    self.kind.invert();
  }
}
