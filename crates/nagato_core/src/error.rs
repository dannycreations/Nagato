use std::io;

use tempfile::PersistError;
use thiserror::Error as ThisError;

#[derive(ThisError, Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
  #[error("Invalid hunk range line: {0}")]
  InvalidHunkRangeLine(String),
  #[error("Invalid hunk range span: {0}")]
  InvalidHunkRangeSpan(String),
  #[error("Malformed hunk header")]
  MalformedHunkHeader,
  #[error("Missing old range in hunk header")]
  MissingOldRange,
  #[error("Missing new range in hunk header")]
  MissingNewRange,
  #[error("Invalid percentage: {0}")]
  InvalidPercentage(String),
  #[error("Invalid file mode: {0}")]
  InvalidFileMode(String),
  #[error("Invalid file header")]
  InvalidFileHeader,
  #[error("Malformed file path")]
  MalformedFilePath,
  #[error("Invalid index line")]
  InvalidIndexLine,
  #[error("Invalid index hash range")]
  InvalidIndexHashRange,
  #[error("Invalid mode line")]
  InvalidModeLine,
  #[error("Invalid similarity line")]
  InvalidSimilarityLine,
  #[error("Invalid binary files line")]
  InvalidBinaryFilesLine,
  #[error("Unexpected line: {0}")]
  UnexpectedLine(String),
  #[error(
    "Hunk line count mismatch for old file. Expected {expected}, got {actual}"
  )]
  HunkLineCountMismatchOld { expected: u32, actual: u32 },
  #[error(
    "Hunk line count mismatch for new file. Expected {expected}, got {actual}"
  )]
  HunkLineCountMismatchNew { expected: u32, actual: u32 },
  #[error("Expected hunk header")]
  ExpectedHunkHeader,
  #[error("Patch has content but no file information")]
  PatchHasContentButNoFileInfo,
  #[error("Unexpected end of patch")]
  UnexpectedEof,
  #[error("Expected a file header, but got something else")]
  ExpectedFileHeader,
}

#[derive(ThisError, Debug)]
pub enum Error {
  #[error(transparent)]
  Io(#[from] io::Error),
  #[error("{0}")]
  Parse(#[from] ParseError),
  #[error("{0}")]
  Apply(#[from] ApplyError),
  #[error("{0}")]
  Message(&'static str),
}

impl From<PersistError> for Error {
  fn from(e: PersistError) -> Self {
    Error::Io(e.error)
  }
}

#[derive(ThisError, Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
  #[error("Could not apply hunk")]
  CouldNotApplyHunk,
}
