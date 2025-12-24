use std::io;

use tempfile::PersistError;
use thiserror::Error as ThisError;

pub mod kind;
pub use kind::ErrorKind;

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
