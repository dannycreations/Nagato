use std::io::ErrorKind as IoErrorKind;

use crate::{error::ErrorKind, Error};

/// Extension trait for byte slices to check for /dev/null.
pub trait IsDevNull {
  fn is_dev_null(&self) -> bool;
}

impl IsDevNull for [u8] {
  #[inline]
  fn is_dev_null(&self) -> bool {
    self == b"/dev/null"
  }
}

/// Extension trait for Result to easily ignore "Not Found" I/O errors.
pub trait IgnoreNotFound {
  fn ignore_not_found(self) -> Self;
}

impl<T> IgnoreNotFound for Result<T, Error>
where
  T: Default,
{
  fn ignore_not_found(self) -> Self {
    match self {
      Err(Error {
        kind: ErrorKind::Io(e),
        ..
      }) if e.kind() == IoErrorKind::NotFound => Ok(T::default()),
      res => res,
    }
  }
}
