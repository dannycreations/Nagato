mod atomic;
mod virtuals;

use std::{
  fmt::Write,
  fs,
  path::{Path, PathBuf},
};

pub use atomic::*;
pub use virtuals::*;

use crate::Error;

pub fn ensure_dir(dir: &Path) -> Result<(), Error> {
  fs::create_dir_all(dir).map_err(Into::into)
}

pub fn get_unique_path(dir: &Path, name: &str) -> PathBuf {
  let mut path = dir.join(name);
  if !path.try_exists().unwrap_or(true) {
    return path;
  }

  let (stem, extension) = name
    .find('.')
    .filter(|&i| i > 0)
    .map(|i| name.split_at(i))
    .unwrap_or((name, ""));

  let mut name_buf = String::with_capacity(name.len() + 10);
  for counter in 1..u32::MAX {
    name_buf.clear();
    let _ = write!(name_buf, "{}-{}{}", stem, counter, extension);

    path.set_file_name(&name_buf);
    if !path.try_exists().unwrap_or(true) {
      return path;
    }
  }
  path
}
