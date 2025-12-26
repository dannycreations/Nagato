use std::{
  env,
  fs::File,
  io::{self, IsTerminal, Read},
  path::PathBuf,
};

use memmap2::Mmap;
use nagato_core::{Error, FileSystem};
use processor::process_patch;

mod args;
mod processor;

pub use args::*;
pub use clap::*;

/// Main entry point for the CLI logic.
pub fn run(cli: &Cli) -> Result<(), Error> {
  // If no files are provided and stdin is a terminal, print help.
  if cli.files.is_empty() && io::stdin().is_terminal() {
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
  // We unify the input source into a collection of byte buffers.
  if cli.files.is_empty() {
    let mut stdin_content = Vec::new();
    io::stdin().read_to_end(&mut stdin_content)?;
    process_patch(&fs, &stdin_content, cli.reverse)?;
  } else {
    for path in &cli.files {
      let file = File::open(path)?;
      // SAFETY: Mmap is used for efficient reading of large patch files.
      let mmap = unsafe { Mmap::map(&file)? };
      process_patch(&fs, &mmap, cli.reverse)?;
    }
  }

  Ok(())
}
