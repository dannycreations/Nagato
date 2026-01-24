use std::ffi::OsString;

use clap::{Parser as ClapParser, Subcommand};

#[derive(ClapParser, Debug)]
#[command(
  author,
  long_about = None,
  disable_version_flag = true,
)]
pub struct Cli {
  #[command(subcommand)]
  pub command: Option<Commands>,

  /// The path to the patch files.
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

#[derive(Subcommand, Debug)]
pub enum Commands {
  /// Trim optional parts from a patch file.
  Trim {
    /// The path to the patch files to trim.
    files: Vec<OsString>,
    /// The directory to store the trim patches.
    #[arg(short, long)]
    directory: Option<OsString>,
  },
  /// Split multi-file patches into independent files.
  Split {
    /// The path to the patch files to split.
    files: Vec<OsString>,
    /// The directory to store the split patches.
    #[arg(short, long)]
    directory: Option<OsString>,
  },
  /// Merge multiple patch files into a single patch file.
  Merge {
    /// The path to the patch files to merge.
    files: Vec<OsString>,
    /// The output path for the merged patch.
    #[arg(short, long)]
    output: Option<OsString>,
  },
}
