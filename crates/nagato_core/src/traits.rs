use std::borrow::Cow;

use crate::Error;

pub trait IsDevNull {
  fn is_dev_null(&self) -> bool;
}

impl IsDevNull for [u8] {
  #[inline]
  fn is_dev_null(&self) -> bool {
    self == b"dev/null" || self == b"/dev/null"
  }
}

impl IsDevNull for Cow<'_, [u8]> {
  #[inline]
  fn is_dev_null(&self) -> bool {
    self.as_ref().is_dev_null()
  }
}

pub trait IgnoreNotFound<T> {
  fn ignore_not_found(self) -> Result<T, Error>;
}

impl<T> IgnoreNotFound<T> for Result<T, Error>
where
  T: Default,
{
  #[inline]
  fn ignore_not_found(self) -> Result<T, Error> {
    let Err(e) = &self else {
      return self;
    };

    if !e.is_not_found() {
      return self;
    }

    Ok(T::default())
  }
}
