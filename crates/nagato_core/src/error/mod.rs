use std::{borrow::Cow, fmt, io::Error as IoError};

use tempfile::PersistError;
use thiserror::Error as ThisError;

mod kind;

pub use kind::*;

/// Core error type for Nagato.
#[derive(ThisError, Debug)]
pub struct Error {
  /// Optional line number where the error occurred.
  pub line: Option<u32>,
  /// Optional file name (target) where the error occurred.
  pub file: Option<Cow<'static, str>>,
  /// Optional origin name (patch file) where the error occurred.
  pub origin: Option<Cow<'static, str>>,
  /// The specific kind of error.
  pub kind: ErrorKind,
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.kind)?;

    let has_file = self.file.is_some();
    let has_origin = self.origin.is_some();
    let has_line = self.line.is_some();

    if has_file || has_origin || has_line {
      write!(f, "\n  ")?;

      if let Some(line) = self.line {
        write!(f, "at ")?;
        match self.origin.as_deref() {
          Some(src) => write!(f, "{src}:{line}"),
          None => write!(f, "<stdin>:{line}"),
        }?;
        if let Some(target) = self.file.as_deref() {
          write!(f, " (applying to {target})")?;
        }
      } else {
        match (self.file.as_deref(), self.origin.as_deref()) {
          (Some(target), Some(src)) => write!(f, "in {target} (from {src})"),
          (Some(target), None) => write!(f, "in {target}"),
          (None, Some(src)) => write!(f, "in {src}"),
          (None, None) => unreachable!(),
        }?;
      }
    }
    Ok(())
  }
}

impl Error {
  /// Create a new error without line or file information.
  #[inline]
  pub const fn new(kind: ErrorKind) -> Self {
    Self {
      kind,
      line: None,
      file: None,
      origin: None,
    }
  }

  /// Create a new error with specific line information.
  #[inline]
  pub const fn with_line(kind: ErrorKind, line: u32) -> Self {
    Self {
      kind,
      line: Some(line),
      file: None,
      origin: None,
    }
  }

  /// Attach a file name (target) to the error.
  #[inline]
  pub fn with_file(mut self, file: impl Into<Cow<'static, str>>) -> Self {
    self.file = Some(file.into());
    self
  }

  /// Attach an origin name (patch file) to the error.
  #[inline]
  pub fn with_origin(mut self, origin: impl Into<Cow<'static, str>>) -> Self {
    self.origin = Some(origin.into());
    self
  }
}

impl From<ErrorKind> for Error {
  /// Convert ErrorKind directly to Error.
  #[inline]
  fn from(kind: ErrorKind) -> Self {
    Self::new(kind)
  }
}

impl From<IoError> for Error {
  /// Automatically wrap I/O errors into the core Error type.
  fn from(e: IoError) -> Self {
    Self::new(ErrorKind::Io(e))
  }
}

impl From<PersistError> for Error {
  /// Automatically wrap persistence errors into the core Error type.
  fn from(e: PersistError) -> Self {
    Self::new(ErrorKind::Persist(e))
  }
}
