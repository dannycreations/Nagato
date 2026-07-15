use std::{env, path::PathBuf};

use nagato_apply::apply_to_fs;
use nagato_core::{Error, FileSystem};

use crate::cmd::{source::PatchSource, Cli};

pub fn process_apply(cli: Cli) -> Result<(), Error> {
  // Determine the root directory for file operations.
  let root = cli
    .directory
    .as_ref()
    .map(PathBuf::from)
    .map(Ok)
    .unwrap_or_else(env::current_dir)?;
  let fs = FileSystem::new(root, cli.check);

  // Process patches from stdin or specified files.
  for source_res in PatchSource::iter(cli.files) {
    let source = source_res?;
    apply_to_fs(&fs, source.content(), cli.reverse)
      .map_err(|e| e.with_origin(source.name().to_string()))?
  }

  Ok(())
}
