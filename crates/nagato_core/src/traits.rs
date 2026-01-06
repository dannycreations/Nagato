use std::io::ErrorKind as IoErrorKind;

use crate::Error;

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
pub trait IgnoreNotFound<T> {
  /// Returns `Ok(T::default())` if the error is a "Not Found" I/O error.
  fn ignore_not_found(self) -> Result<T, Error>;
}

impl<T> IgnoreNotFound<T> for Result<T, Error>
where
  T: Default,
{
  #[inline]
  fn ignore_not_found(self) -> Result<T, Error> {
    match self {
      Err(ref e) if e.kind.io_kind() == Some(IoErrorKind::NotFound) => {
        Ok(T::default())
      }
      res => res,
    }
  }
}
