use std::{
  env,
  io::{stdin, IsTerminal},
  path::PathBuf,
};

use clap::CommandFactory;
use nagato_apply::Parser;
use nagato_core::{Error, FileSystem};

use crate::cmd::{
  merge::process_merge, source::PatchSource, split::process_split,
  trim::process_trim,
};

mod args;
mod merge;
mod source;
mod split;
mod trim;
mod utils;

pub use args::*;

pub fn run(cli: Cli) -> Result<(), Error> {
  if let Some(Commands::Trim { files, directory }) = cli.command {
    return process_trim(files, directory.map(PathBuf::from));
  }
  if let Some(Commands::Split { files, directory }) = cli.command {
    return process_split(files, directory.map(PathBuf::from));
  }
  if let Some(Commands::Merge { files, output }) = cli.command {
    return process_merge(files, output.map(PathBuf::from));
  }

  // If no files are provided and stdin is a terminal, print help.
  if cli.files.is_empty() && stdin().is_terminal() {
    Cli::command().print_help().expect("failed to print help");
    return Ok(());
  }

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
    Parser::apply_to_fs(&fs, source.content(), cli.reverse)
      .map_err(|e| e.with_origin(source.name().to_string()))?;
  }

  Ok(())
}
