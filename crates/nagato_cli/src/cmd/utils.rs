use nagato_apply::{Parser, Patch};
use nagato_core::Error;

use super::source::PatchSource;

pub fn parse_patches(
  source: &PatchSource,
) -> Result<impl Iterator<Item = Result<Patch<'_>, Error>>, Error> {
  Ok(
    Parser::new(source.content()).map(move |res| {
      res.map_err(|e| e.with_origin(source.name().to_string()))
    }),
  )
}
