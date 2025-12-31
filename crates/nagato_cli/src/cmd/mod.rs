use std::{
  env,
  io::{stdin, IsTerminal},
  path::PathBuf,
};

use nagato_core::{Error, FileSystem};
use processor::process_patch;
use source::PatchSource;
use trimmer::process_trim;

mod args;
mod processor;
mod source;
mod trimmer;

pub use args::*;
pub use clap::*;

/// Main entry point for the CLI logic.
pub fn run(cli: &Cli) -> Result<(), Error> {
  if let Some(command) = &cli.command {
    match command {
      Commands::Trim { files } => {
        return process_trim(files);
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
    process_patch(&fs, source.content(), cli.reverse, cli.check)
      .map_err(|e| e.with_file(source.name().to_string()))?;
  }

  Ok(())
}
