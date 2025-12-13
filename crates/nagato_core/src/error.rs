use std::io;

use tempfile::PersistError;
use thiserror::Error as ThisError;

// By deriving `Clone`, we can handle errors more flexibly, especially in the
// parser where we need to peek at tokens and potentially clone errors.
// `PartialEq` and `Eq` are also derived for easier testing and comparison.
#[derive(ThisError, Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
  // Storing the I/O error as a String makes the `Error` enum cloneable,
  // as `std::io::Error` itself is not cloneable. This is a pragmatic
  // trade-off for cleaner error handling in the parser.
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

impl From<io::Error> for ErrorKind {
  fn from(e: io::Error) -> Self {
    ErrorKind::Io(e.to_string())
  }
}

impl From<PersistError> for ErrorKind {
  fn from(e: PersistError) -> Self {
    ErrorKind::Io(e.error.to_string())
  }
}
