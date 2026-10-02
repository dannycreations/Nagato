mod base85;
mod delta;

pub(crate) use base85::{decode_base85, new_base85_decoder};
pub use delta::apply_delta;
