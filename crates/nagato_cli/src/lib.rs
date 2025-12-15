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
  // This function encapsulates the core logic of parsing and applying a patch,
  // allowing it to be reused for both file and stdin inputs.
  for patch in Parser::new(patch_content) {
    patch_file(fs, patch?, reverse)?;
  }
  Ok(())
}

pub fn run(cli: &Cli) -> Result<(), Error> {
  // If no file is given and we are in an interactive terminal, print the help
  // message. This is a better user experience than hanging while waiting for
  // stdin that will never come.
  if cli.file.is_none() && io::stdin().is_terminal() {
    Cli::command().print_help().unwrap();
    return Ok(());
  }

  // Using `env::current_dir()` is more explicit and robust than relying on
  // a relative path like ".". It clearly establishes the current working
  // directory as the default root for applying patches, improving clarity
  // without changing behavior.
  let root = if let Some(dir) = &cli.directory {
    PathBuf::from(dir)
  } else {
    env::current_dir()?
  };
  let mut fs = OsFileSystem::new(root);

  // Using a `match` expression here is more idiomatic and clearly expresses
  // the logic for handling either a file or stdin as the patch source.
  match &cli.file {
    Some(path) => {
      let file = File::open(path)?;
      // Using memory-mapping is highly efficient for large files, as it avoids
      // loading the entire content into RAM. The OS handles paging transparently.
      let mmap = unsafe { Mmap::map(&file)? };
      process_patch(&mut fs, &mmap, cli.reverse)
    }
    None => {
      // Reading from stdin allows the tool to be used in pipelines, which is a
      // common and powerful pattern in command-line utilities.
      let mut stdin_content = Vec::new();
      io::stdin().read_to_end(&mut stdin_content)?;
      process_patch(&mut fs, &stdin_content, cli.reverse)
    }
  }
}
