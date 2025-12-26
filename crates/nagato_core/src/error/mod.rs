use std::io;

use tempfile::PersistError;
use thiserror::Error as ThisError;

mod kind;

pub use kind::*;

/// Core error type for Nagato.
#[derive(ThisError, Debug)]
#[error("{kind}")]
pub struct Error {
  /// Optional line number where the error occurred.
  pub line: Option<u32>,
  /// The specific kind of error.
  #[source]
  pub kind: ErrorKind,
}

impl Error {
  /// Create a new error without line information.
  #[inline]
  pub const fn new(kind: ErrorKind) -> Self {
    Self { kind, line: None }
  }

  /// Create a new error with specific line information.
  #[inline]
  pub const fn with_line(kind: ErrorKind, line: u32) -> Self {
    Self {
      kind,
      line: Some(line),
    }
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
