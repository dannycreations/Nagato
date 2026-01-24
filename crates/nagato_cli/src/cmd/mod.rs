use std::{
  env,
  io::{stdin, IsTerminal},
  path::PathBuf,
};

use merge::process_merge;
use nagato_apply::Parser;
use nagato_core::{Error, FileSystem};
use source::PatchSource;
use split::process_split;
use trim::process_trim;

mod args;
mod merge;
mod source;
mod split;
mod trim;
mod utils;

pub use args::*;
pub use clap::*;

pub fn run(cli: &Cli) -> Result<(), Error> {
  if let Some(command) = &cli.command {
    match command {
      Commands::Trim { files, directory } => {
        return process_trim(files, directory.as_ref().map(PathBuf::from));
      }
      Commands::Split { files, directory } => {
        return process_split(files, directory.as_ref().map(PathBuf::from));
      }
      Commands::Merge { files, output } => {
        return process_merge(files, output.as_ref().map(PathBuf::from));
      }
    }
  }

  // If no files are provided and stdin is a terminal, print help.
  if cli.files.is_empty() && stdin().is_terminal() {
    Cli::command().print_help().unwrap();
    return Ok(());
  }

  // Determine the root directory for file operations.
  let root = if let Some(dir) = &cli.directory {
    PathBuf::from(dir)
  } else {
    env::current_dir()?
  };
  let fs = FileSystem::new(root);

  // Process patches from stdin or specified files.
  for source_res in PatchSource::iter(cli.files.clone()) {
    let source = source_res?;
    Parser::apply_to_fs(&fs, source.content(), cli.reverse, cli.check)
      .map_err(|e| e.with_origin(source.name().to_string()))?;
  }

  Ok(())
}
