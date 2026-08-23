use std::env;

use nagato_apply::apply_to_fs;
use nagato_core::{Error, FileSystem};

use crate::cmd::{source::PatchSource, Cli};

pub fn process_apply(cli: Cli) -> Result<(), Error> {
  let root = match cli.directory {
    Some(dir) => dir,
    None => env::current_dir()?,
  };
  let fs = FileSystem::new(root, cli.check);

  for source_res in PatchSource::iter(cli.files) {
    let source = source_res?;
    apply_to_fs(&fs, source.content(), cli.reverse)
      .map_err(|e| e.with_origin(source.name().to_string()))?
  }

  Ok(())
}
