use flate2::read::ZlibDecoder;

mod base85;
mod delta;

pub use base85::*;
pub use delta::*;

pub fn new_base85_decoder<'a>(
  lines: &'a [&'a [u8]],
) -> ZlibDecoder<Base85Reader<'a>> {
  ZlibDecoder::new(Base85Reader::new(lines))
}
