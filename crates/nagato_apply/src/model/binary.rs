#[derive(Debug, Clone, PartialEq, Copy)]
pub enum BinaryKind {
  Literal,
  Delta,
}

#[derive(Debug, PartialEq, Clone)]
pub struct BinaryFragment<'a> {
  pub kind: BinaryKind,
  pub size: u64,
  pub data: Vec<&'a [u8]>,
}
