#[derive(Debug, Clone, PartialEq, Copy)]
pub enum BinaryPatchKind {
  Literal,
  Delta,
}

#[derive(Debug, PartialEq, Clone)]
pub struct BinaryFragment<'a> {
  pub kind: BinaryPatchKind,
  pub size: u64,
  pub data: Vec<&'a [u8]>,
}
