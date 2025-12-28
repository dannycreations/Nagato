use std::{io::Error as IoError, sync::Arc};

use tempfile::PersistError;
use thiserror::Error as ThisError;

#[derive(ThisError, Debug, Clone)]
pub enum ErrorKind {
  #[error("I/O error")]
  Io(#[from] ArcIoError),
  #[error("Failed to persist temporary file")]
  Persist(#[from] ArcPersistError),

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
  #[error("Can't open patch '{0}'\n  {1}")]
  CantOpenPatch(String, ArcIoError),
}

#[derive(Debug, Clone, ThisError)]
#[error(transparent)]
pub struct ArcIoError(pub Arc<IoError>);

impl From<IoError> for ArcIoError {
  fn from(e: IoError) -> Self {
    Self(Arc::new(e))
  }
}

#[derive(Debug, Clone, ThisError)]
#[error(transparent)]
pub struct ArcPersistError(pub Arc<PersistError>);

impl From<PersistError> for ArcPersistError {
  fn from(e: PersistError) -> Self {
    Self(Arc::new(e))
  }
}
