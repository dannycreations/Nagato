use std::ffi::OsString;

use clap::Parser as ClapParser;

#[derive(ClapParser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
  /// The path to the patch file(s).
  pub files: Vec<OsString>,
  /// Apply the patch in reverse.
  #[arg(short, long)]
  pub reverse: bool,
  /// The directory to run the patch in.
  #[arg(short, long)]
  pub directory: Option<OsString>,
}
