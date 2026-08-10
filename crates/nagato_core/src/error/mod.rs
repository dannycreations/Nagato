use std::{
  fmt,
  io::{Error as IoError, ErrorKind as IoErrorKind},
};

use tempfile::PersistError;
use thiserror::Error as ThisError;

mod kind;

pub use kind::*;

#[derive(ThisError, Debug)]
pub struct Error {
  pub line: Option<u32>,
  pub file: Option<Box<str>>,
  pub origin: Option<Box<str>>,
  pub kind: ErrorKind,
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.kind)?;

    if let Some(line) = self.line {
      let origin = self.origin.as_deref().unwrap_or("<stdin>");
      write!(f, "\n  at {origin}:{line}")?;

      if let Some(file) = self.file.as_deref() {
        write!(f, " (applying to {file})")?;
      }

      return Ok(());
    }

    let origin = self.origin.as_deref();
    let file = self.file.as_deref();

    match (origin, file) {
      (Some(origin), Some(file)) => write!(f, "\n  in {file} (from {origin})"),
      (Some(origin), None) => write!(f, "\n  in {origin}"),
      (None, Some(file)) => write!(f, "\n  in {file}"),
      (None, None) => Ok(()),
    }
  }
}

impl Error {
  #[inline]
  pub const fn new(kind: ErrorKind) -> Self {
    Self {
      kind,
      line: None,
      file: None,
      origin: None,
    }
  }

  #[inline]
  pub const fn with_line(kind: ErrorKind, line: u32) -> Self {
    Self {
      kind,
      line: Some(line),
      file: None,
      origin: None,
    }
  }

  #[inline]
  pub fn with_file(mut self, file: impl Into<Box<str>>) -> Self {
    self.file = Some(file.into());
    self
  }

  #[inline]
  pub fn with_origin(mut self, origin: impl Into<Box<str>>) -> Self {
    self.origin = Some(origin.into());
    self
  }

  #[inline]
  pub fn is_not_found(&self) -> bool {
    // Error classification for missing resources is determined by inspecting the underlying I/O error kind for a NotFound status.
    self.kind.io_kind() == Some(IoErrorKind::NotFound)
  }
}

impl From<ErrorKind> for Error {
  #[inline]
  fn from(kind: ErrorKind) -> Self {
    Self::new(kind)
  }
}

impl From<IoError> for Error {
  fn from(e: IoError) -> Self {
    Self::new(ErrorKind::Io(e))
  }
}

impl From<PersistError> for Error {
  fn from(e: PersistError) -> Self {
    Self::new(ErrorKind::Persist(e))
  }
}
