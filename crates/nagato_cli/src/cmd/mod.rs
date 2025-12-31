use std::{
  env,
  fs::File,
  io::{stdin, IsTerminal, Read},
  path::PathBuf,
};

use memmap2::Mmap;
use nagato_core::{Error, ErrorKind, FileSystem};
use processor::process_patch;
use trimmer::process_trim;

mod args;
mod processor;
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
  // We unify the input source into a collection of byte buffers.
  if cli.files.is_empty() {
    let mut stdin_content = Vec::new();
    stdin().read_to_end(&mut stdin_content)?;
    process_patch(&fs, &stdin_content, cli.reverse, cli.check)
      .map_err(|e| e.with_file("<stdin>".into()))?;
  } else {
    for path in &cli.files {
      let file_name = path.to_string_lossy().to_string();
      let file = File::open(path).map_err(|e| {
        Error::new(ErrorKind::CantOpenPatch(file_name.clone(), e))
      })?;
      // SAFETY: Mmap is used for efficient reading of large patch files.
      let mmap = unsafe { Mmap::map(&file)? };
      process_patch(&fs, &mmap, cli.reverse, cli.check)
        .map_err(|e| e.with_file(file_name))?;
    }
  }

  Ok(())
}
