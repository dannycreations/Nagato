pub mod base85;
pub mod delta;

pub use base85::{decode_base85, Base85Reader};
pub use delta::apply_delta;
use flate2::read::ZlibDecoder;

pub fn new_base85_decoder<'a>(
  lines: &'a [&'a [u8]],
) -> ZlibDecoder<Base85Reader<'a>> {
  ZlibDecoder::new(Base85Reader::new(lines))
}
