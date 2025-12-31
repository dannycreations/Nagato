use std::io::Error as IoError;

use tempfile::PersistError;
use thiserror::Error as ThisError;

#[derive(ThisError, Debug)]
pub enum ErrorKind {
  #[error("I/O error")]
  Io(#[from] IoError),
  #[error("Failed to persist temporary file")]
  Persist(#[from] PersistError),

  #[error("{0}")]
  Parse(#[from] ParseErrorKind),

  #[error("{0}")]
  Apply(#[from] ApplyErrorKind),

  #[error("Invalid path")]
  InvalidPath,
  #[error("Can't open patch '{0}'\n  {1}")]
  CantOpenPatch(String, IoError),
}

#[derive(ThisError, Debug, PartialEq, Eq)]
pub enum ParseErrorKind {
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
}

#[derive(ThisError, Debug, PartialEq, Eq)]
pub enum ApplyErrorKind {
  #[error("Could not apply hunk")]
  CouldNotApplyHunk,
  #[error("Binary patch content is not supported")]
  UnsupportedBinaryPatch,
  #[error("Invalid binary patch data")]
  InvalidBinaryPatch,
  #[error("Binary patch source length mismatch")]
  BinaryPatchSourceMismatch,
}

impl PartialEq for ErrorKind {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::Io(a), Self::Io(b)) => a.kind() == b.kind(),
      (Self::Persist(_), Self::Persist(_)) => true,
      (Self::Parse(a), Self::Parse(b)) => a == b,
      (Self::Apply(a), Self::Apply(b)) => a == b,
      (Self::InvalidPath, Self::InvalidPath) => true,
      (Self::CantOpenPatch(a, ae), Self::CantOpenPatch(b, be)) => {
        a == b && ae.kind() == be.kind()
      }
      _ => false,
    }
  }
}

impl Eq for ErrorKind {}

#[allow(non_upper_case_globals)]
impl ErrorKind {
  pub const InvalidHunkRangeLine: Self =
    Self::Parse(ParseErrorKind::InvalidHunkRangeLine);
  pub const InvalidHunkRangeSpan: Self =
    Self::Parse(ParseErrorKind::InvalidHunkRangeSpan);
  pub const MissingOldRange: Self =
    Self::Parse(ParseErrorKind::MissingOldRange);
  pub const MissingNewRange: Self =
    Self::Parse(ParseErrorKind::MissingNewRange);
  pub const InvalidPercentage: Self =
    Self::Parse(ParseErrorKind::InvalidPercentage);
  pub const InvalidFileMode: Self =
    Self::Parse(ParseErrorKind::InvalidFileMode);
  pub const InvalidFileHeader: Self =
    Self::Parse(ParseErrorKind::InvalidFileHeader);
  pub const InvalidIndexLine: Self =
    Self::Parse(ParseErrorKind::InvalidIndexLine);
  pub const InvalidIndexHashRange: Self =
    Self::Parse(ParseErrorKind::InvalidIndexHashRange);
  pub const InvalidBinaryFilesLine: Self =
    Self::Parse(ParseErrorKind::InvalidBinaryFilesLine);
  pub const UnexpectedLine: Self = Self::Parse(ParseErrorKind::UnexpectedLine);
  pub const HunkLineCountMismatch: Self =
    Self::Parse(ParseErrorKind::HunkLineCountMismatch);
  pub const ExpectedHunkHeader: Self =
    Self::Parse(ParseErrorKind::ExpectedHunkHeader);
  pub const PatchHasContentButNoFileInfo: Self =
    Self::Parse(ParseErrorKind::PatchHasContentButNoFileInfo);
  pub const UnexpectedEof: Self = Self::Parse(ParseErrorKind::UnexpectedEof);

  pub const CouldNotApplyHunk: Self =
    Self::Apply(ApplyErrorKind::CouldNotApplyHunk);
  pub const UnsupportedBinaryPatch: Self =
    Self::Apply(ApplyErrorKind::UnsupportedBinaryPatch);
  pub const InvalidBinaryPatch: Self =
    Self::Apply(ApplyErrorKind::InvalidBinaryPatch);
  pub const BinaryPatchSourceMismatch: Self =
    Self::Apply(ApplyErrorKind::BinaryPatchSourceMismatch);
}
