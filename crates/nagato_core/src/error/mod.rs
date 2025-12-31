use std::{fmt, io::Error as IoError};

use anyhow::Error as AnyhowError;
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
  pub kind: ErrorKind,
  /// Wrapped context error.
  pub context: Option<AnyhowError>,
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.kind)?;
    if let Some(ctx) = &self.context {
      write!(f, ": {ctx:?}")?;
    }
    if let Some(line) = self.line {
      write!(f, "\n  at ")?;
      match &self.file {
        Some(file) => write!(f, "{file}:{line}"),
        None => write!(f, "<stdin>:{line}"),
      }?;
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
      context: None,
    }
  }

  /// Create a new error with specific line information.
  #[inline]
  pub const fn with_line(kind: ErrorKind, line: u32) -> Self {
    Self {
      kind,
      line: Some(line),
      file: None,
      context: None,
    }
  }

  /// Attach a file name to the error.
  pub fn with_file(mut self, file: String) -> Self {
    self.file = Some(file);
    self
  }

  /// Attach an anyhow context to the error.
  pub fn with_context(mut self, context: AnyhowError) -> Self {
    self.context = Some(context);
    self
  }
}

impl From<AnyhowError> for Error {
  /// Convert an anyhow error to our core Error.
  /// If it's already an Error, we downcast it to avoid double-wrapping.
  fn from(e: AnyhowError) -> Self {
    e.downcast::<Self>().unwrap_or_else(|e| {
      Self::new(ErrorKind::Io(IoError::other(e.to_string()))).with_context(e)
    })
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
