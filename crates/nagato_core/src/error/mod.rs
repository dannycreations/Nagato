use std::{fmt, io};

use tempfile::PersistError;
use thiserror::Error as ThisError;

mod kind;

pub use kind::*;

/// Core error type for Nagato.
#[derive(ThisError, Debug)]
pub struct Error {
  /// Optional line number where the error occurred.
  pub line: Option<u32>,
  /// Optional file name where the error occurred.
  pub file: Option<String>,
  /// The specific kind of error.
  #[source]
  pub kind: ErrorKind,
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.kind)?;
    if let Some(line) = self.line {
      write!(f, "\n  at ")?;
      if let Some(file) = &self.file {
        write!(f, "{file}:")?;
      } else {
        write!(f, "<stdin>:")?;
      }
      write!(f, "{line}")?;
    } else if let Some(file) = &self.file {
      write!(f, "\n  in {file}")?;
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
    }
  }

  /// Create a new error with specific line information.
  #[inline]
  pub const fn with_line(kind: ErrorKind, line: u32) -> Self {
    Self {
      kind,
      line: Some(line),
      file: None,
    }
  }

  /// Attach a file name to the error.
  pub fn with_file(mut self, file: String) -> Self {
    self.file = Some(file);
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

impl From<io::Error> for Error {
  /// Automatically wrap I/O errors into the core Error type.
  fn from(e: io::Error) -> Self {
    Self::new(ErrorKind::Io(e))
  }
}

impl From<PersistError> for Error {
  /// Automatically wrap persistence errors into the core Error type.
  fn from(e: PersistError) -> Self {
    Self::new(ErrorKind::Persist(e))
  }
}
