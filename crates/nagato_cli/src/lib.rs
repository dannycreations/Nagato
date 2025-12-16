use std::{
  env,
  ffi::OsString,
  fs::File,
  io::{self, IsTerminal, Read},
  path::PathBuf,
};

pub use clap::{CommandFactory, Parser as ClapParser};
use memmap2::Mmap;
use nagato_apply::{patch_file, Parser};
use nagato_core::{error::Error, fs::OsFileSystem};

#[derive(ClapParser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
  /// The path to the patch file. If not provided, the patch will be read from stdin.
  pub file: Option<OsString>,
  /// Apply the patch in reverse.
  #[arg(short, long)]
  reverse: bool,
  /// The directory to run the patch in. Defaults to the current directory.
  #[arg(short, long)]
  directory: Option<OsString>,
}

fn process_patch(
  fs: &mut OsFileSystem,
  patch_content: &[u8],
  reverse: bool,
) -> Result<(), Error> {
  for patch in Parser::new(patch_content) {
    patch_file(fs, patch?, reverse)?;
  }
  Ok(())
}

pub fn run(cli: &Cli) -> Result<(), Error> {
  if cli.file.is_none() && io::stdin().is_terminal() {
    Cli::command().print_help().unwrap();
    return Ok(());
  }

  let root = if let Some(dir) = &cli.directory {
    PathBuf::from(dir)
  } else {
    env::current_dir()?
  };
  let mut fs = OsFileSystem::new(root);

  match &cli.file {
    Some(path) => {
      let file = File::open(path)?;
      let mmap = unsafe { Mmap::map(&file)? };
      process_patch(&mut fs, &mmap, cli.reverse)
    }
    None => {
      let mut stdin_content = Vec::new();
      io::stdin().read_to_end(&mut stdin_content)?;
      process_patch(&mut fs, &stdin_content, cli.reverse)
    }
  }
}
