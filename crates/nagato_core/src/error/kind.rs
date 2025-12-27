use std::io;

use tempfile::PersistError;
use thiserror::Error as ThisError;

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
  #[error("Can't open patch '{0}'\n  {1}")]
  CantOpenPatch(String, io::Error),
}
