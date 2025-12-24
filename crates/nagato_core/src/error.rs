use std::io;

use tempfile::PersistError;
use thiserror::Error as ThisError;

#[derive(ThisError, Debug)]
#[error("{kind}")]
pub struct Error {
  pub line: Option<u32>,
  #[source]
  pub kind: ErrorKind,
}

#[derive(ThisError, Debug)]
pub enum ErrorKind {
  #[error("I/O error")]
  Io(#[from] io::Error),
  #[error("Failed to persist temporary file")]
  Persist(#[from] PersistError),

  /// Parse errors
  #[error("Invalid hunk range line")]
  InvalidHunkRangeLine,
  #[error("Invalid hunk range span")]
  InvalidHunkRangeSpan,
  #[error("Missing old range in hunk header")]
  MissingOldRange,
  #[error("Missing new range in hunk header")]
  MissingNewRange,
  #[error("Invalid percentage")]
  InvalidPercentage,
  #[error("Invalid file mode")]
  InvalidFileMode,
  #[error("Invalid file header")]
  InvalidFileHeader,
  #[error("Invalid index line")]
  InvalidIndexLine,
  #[error("Invalid index hash range")]
  InvalidIndexHashRange,
  #[error("Invalid binary files line")]
  InvalidBinaryFilesLine,
  #[error("Unexpected line")]
  UnexpectedLine,
  #[error("Hunk line count mismatch")]
  HunkLineCountMismatch,
  #[error("Expected hunk header")]
  ExpectedHunkHeader,
  #[error("Patch has content but no file information")]
  PatchHasContentButNoFileInfo,
  #[error("Unexpected end of patch")]
  UnexpectedEof,

  /// Apply errors
  #[error("Could not apply hunk")]
  CouldNotApplyHunk,

  /// Other errors
  #[error("Binary patch content is not supported")]
  UnsupportedBinaryPatch,
  #[error("Invalid binary patch data")]
  InvalidBinaryPatch,
  #[error("Binary patch source length mismatch")]
  BinaryPatchSourceMismatch,
  #[error("Invalid path")]
  InvalidPath,
}

impl Error {
  /// Create a new error without line information.
  pub const fn new(kind: ErrorKind) -> Self {
    Self { kind, line: None }
  }

  /// Create a new error with specific line information.
  pub const fn with_line(kind: ErrorKind, line: u32) -> Self {
    Self {
      kind,
      line: Some(line),
    }
  }
}

impl From<io::Error> for Error {
  fn from(e: io::Error) -> Self {
    Self::new(ErrorKind::Io(e))
  }
}

impl From<PersistError> for Error {
  fn from(e: PersistError) -> Self {
    Self::new(ErrorKind::Persist(e))
  }
}
