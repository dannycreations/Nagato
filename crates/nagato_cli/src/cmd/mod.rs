use std::{
  io::{stdin, IsTerminal},
  path::PathBuf,
};

use clap::CommandFactory;
use nagato_core::Error;

use crate::cmd::{
  apply::process_apply, merge::process_merge, split::process_split,
  trim::process_trim,
};

mod apply;
mod args;
mod merge;
mod source;
mod split;
mod trim;
mod utils;

pub use args::*;

pub fn execute(cli: Cli) -> Result<(), Error> {
  match cli.command {
    Some(Commands::Trim { files, directory }) => {
      process_trim(files, directory.map(PathBuf::from))
    }
    Some(Commands::Split { files, directory }) => {
      process_split(files, directory.map(PathBuf::from))
    }
    Some(Commands::Merge { files, output }) => {
      process_merge(files, output.map(PathBuf::from))
    }
    None => {
      // Default to "apply" if no subcommand is provided.
      // If no files are provided and stdin is a terminal, print help.
      if cli.files.is_empty() && stdin().is_terminal() {
        Cli::command().print_help().expect("failed to print help");
        return Ok(());
      }
      process_apply(cli)
    }
  }
}
