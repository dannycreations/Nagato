use std::{ffi::OsString, fs::File, path::Path};

use clap::{CommandFactory, Parser as ClapParser};
use memmap2::Mmap;
use nagato_apply::{patch_file, Parser};
use nagato_core::fs::OsFileSystem;

#[derive(ClapParser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
  file: Option<OsString>,
  #[arg(short, long)]
  reverse: bool,
  #[arg(short, long)]
  directory: Option<OsString>,
}

pub fn run() -> Result<(), nagato_core::error::Error> {
  let cli = Cli::parse();
  let root = cli.directory.map_or_else(
    || Path::new(".").to_path_buf(),
    |os_string| os_string.into(),
  );
  let mut fs = OsFileSystem::new(root);

  if let Some(path) = &cli.file {
    let file = File::open(path)?;
    // Using memory-mapping is highly efficient for large files, as it avoids
    // loading the entire content into RAM. The OS handles paging transparently.
    let mmap = unsafe { Mmap::map(&file)? };
    for patch in Parser::new(&mmap) {
      patch_file(&mut fs, patch?, cli.reverse)?;
    }
    Ok(())
  } else {
    Cli::command().print_help().unwrap();
    std::process::exit(0);
  }
}
