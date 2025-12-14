use std::io;

use tempfile::PersistError;
use thiserror::Error as ThisError;

#[derive(ThisError, Debug)]
#[error("{kind}")]
pub struct Error {
  pub line: Option<u64>,
  #[source]
  pub kind: ErrorKind,
}

#[derive(ThisError, Debug)]
pub enum ErrorKind {
  #[error("I/O error")]
  Io(#[from] io::Error),
  #[error("Failed to persist temporary file")]
  Persist(#[from] PersistError),

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

// By implementing `From<io::Error>` for `Error`, we enable the `?` operator
// to automatically convert standard I/O errors into our custom error type.
// This is a common pattern in Rust for ergonomic error handling. It works by
// delegating the conversion to the `ErrorKind`'s `From` implementation.
impl From<io::Error> for Error {
  fn from(e: io::Error) -> Self {
    Error {
      line: None,
      kind: e.into(),
    }
  }
}

// Similarly, this implementation handles errors that can occur when persisting
// a temporary file, ensuring they are also wrapped in our custom `Error` type.
impl From<PersistError> for Error {
  fn from(e: PersistError) -> Self {
    Error {
      line: None,
      kind: e.into(),
    }
  }
}
