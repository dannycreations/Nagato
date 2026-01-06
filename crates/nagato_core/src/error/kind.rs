use core::mem::discriminant;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};

use tempfile::PersistError;
use thiserror::Error as ThisError;

/// Specific types of errors that can occur in Nagato.
/// Flattened to reduce boilerplate and simplify matching.
#[derive(ThisError, Debug)]
pub enum ErrorKind {
  #[error("I/O error")]
  Io(#[from] IoError),
  #[error("Failed to persist temporary file")]
  Persist(#[from] PersistError),

  // Parse Errors
  #[error("Invalid hunk range")]
  InvalidHunkRange,
  #[error("Missing range in hunk header")]
  MissingRange,
  #[error("Invalid percentage")]
  InvalidPercentage,
  #[error("Invalid file mode")]
  InvalidFileMode,
  #[error("Invalid file header")]
  InvalidFileHeader,
  #[error("Invalid index header")]
  InvalidIndexHeader,
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

  // Apply Errors
  #[error("Could not apply hunk")]
  CouldNotApplyHunk,
  #[error("Binary patch content is not supported")]
  UnsupportedBinaryPatch,
  #[error("Invalid binary patch data")]
  InvalidBinaryPatch,
  #[error("Binary patch source length mismatch")]
  BinaryPatchSourceMismatch,

  #[error("Invalid path")]
  InvalidPath,
  #[error("Can't open patch '{0}'\n  {1}")]
  CantOpenPatch(Box<str>, IoError),
}

impl Eq for ErrorKind {}

impl PartialEq for ErrorKind {
  #[inline]
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::Io(a), Self::Io(b)) => a.kind() == b.kind(),
      (Self::CantOpenPatch(a, ae), Self::CantOpenPatch(b, be)) => {
        a == b && ae.kind() == be.kind()
      }
      // Use discriminant to compare variants without data.
      // This reduces maintenance as new data-less variants won't need manual eq updates.
      (a, b) => discriminant(a) == discriminant(b),
    }
  }
}

impl ErrorKind {
  /// Returns the I/O error kind if this is an I/O error.
  pub fn io_kind(&self) -> Option<IoErrorKind> {
    match self {
      Self::Io(e) => Some(e.kind()),
      Self::CantOpenPatch(_, e) => Some(e.kind()),
      _ => None,
    }
  }
}
