use std::io;

use tempfile::PersistError;
use thiserror::Error as ThisError;

// I'm introducing a new top-level `Error` struct. This will wrap the `ErrorKind`
// and will be the standard error type returned from fallible functions.
// This makes error handling more consistent across the application.
#[derive(ThisError, Debug, Clone, PartialEq, Eq)]
#[error("{kind}")]
pub struct Error {
  pub line: Option<u64>,
  pub kind: ErrorKind,
}

// The `ErrorKind` enum now represents the specific type of error that occurred,
// without needing to carry the line number itself.
#[derive(ThisError, Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
  #[error("I/O error: {0}")]
  Io(String),

  // Parse errors
  #[error("Invalid hunk range line: {0}")]
  InvalidHunkRangeLine(String),
  #[error("Invalid hunk range span: {0}")]
  InvalidHunkRangeSpan(String),
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
  #[error("Invalid index line")]
  InvalidIndexLine,
  #[error("Invalid index hash range")]
  InvalidIndexHashRange,
  #[error("Invalid binary files line")]
  InvalidBinaryFilesLine,
  #[error("Unexpected line: {0}")]
  UnexpectedLine(String),
  #[error("Hunk line count mismatch")]
  HunkLineCountMismatch,
  #[error("Expected hunk header")]
  ExpectedHunkHeader,
  #[error("Patch has content but no file information")]
  PatchHasContentButNoFileInfo,
  #[error("Unexpected end of patch")]
  UnexpectedEof,

  // Apply errors
  #[error("Could not apply hunk")]
  CouldNotApplyHunk,

  // Other errors
  #[error("Binary files are not supported")]
  BinaryFilesNotSupported,
}

impl From<io::Error> for Error {
  fn from(e: io::Error) -> Self {
    Error {
      line: None,
      kind: ErrorKind::Io(e.to_string()),
    }
  }
}

impl From<PersistError> for Error {
  fn from(e: PersistError) -> Self {
    Error {
      line: None,
      kind: ErrorKind::Io(e.error.to_string()),
    }
  }
}
