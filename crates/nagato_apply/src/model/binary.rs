#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum BinaryKind {
  Literal = 0,
  Delta = 1,
}

#[derive(Debug, PartialEq, Clone)]
pub struct BinaryFragment {
  pub kind: BinaryKind,
  pub size: u64,
  pub data_start: u32,
  pub data_len: u32,
}
