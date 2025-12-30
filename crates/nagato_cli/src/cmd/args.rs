use std::ffi::OsString;

use clap::Parser as ClapParser;

#[derive(ClapParser, Debug)]
#[command(
  author,
  long_about = None,
  disable_version_flag = true,
)]
pub struct Cli {
  /// The path to the patch file(s).
  pub files: Vec<OsString>,
  /// Print version information.
  #[arg(short, long)]
  pub version: bool,
  /// Apply the patch in reverse.
  #[arg(short, long)]
  pub reverse: bool,
  /// Check if the patch is applicable.
  #[arg(short, long)]
  pub check: bool,
  /// The directory to run the patch in.
  #[arg(short, long)]
  pub directory: Option<OsString>,
}
