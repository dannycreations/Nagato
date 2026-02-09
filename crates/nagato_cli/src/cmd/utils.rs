use nagato_apply::{Parser, Patch};
use nagato_core::Error;

use super::source::PatchSource;

pub fn parse_patches(source: &PatchSource) -> Result<Vec<Patch<'_>>, Error> {
  Parser::new(source.content())
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.with_origin(source.name().to_string()))
}
