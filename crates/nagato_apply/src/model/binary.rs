#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum BinaryKind {
  Literal = 0,
  Delta = 1,
}

#[derive(Debug, PartialEq, Clone)]
pub struct BinaryFragment<'a> {
  pub kind: BinaryKind,
  pub size: u64,
  pub data: Vec<&'a [u8]>,
}
