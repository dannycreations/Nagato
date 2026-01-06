use std::{
  ffi::OsString,
  fs::File,
  io::{stdin, Read},
};

use memmap2::Mmap;
use nagato_core::{Error, ErrorKind};

/// Represents a source of patch data.
pub enum PatchSource {
  Stdin(Vec<u8>),
  File { name: Box<str>, content: Mmap },
}

impl PatchSource {
  /// Returns an iterator over patch sources from the provided list of files, or from stdin if empty.
  /// This allows processing patches one by one without loading all of them into memory at once.
  pub fn iter(
    files: Vec<OsString>,
  ) -> Box<dyn Iterator<Item = Result<Self, Error>>> {
    if files.is_empty() {
      let mut content = Vec::new();
      let res = stdin()
        .read_to_end(&mut content)
        .map(|_| Self::Stdin(content))
        .map_err(Error::from);
      return Box::new(std::iter::once(res));
    }

    Box::new(files.into_iter().map(|path| {
      let file_name: Box<str> = path.to_string_lossy().into();
      let file = File::open(&path).map_err(|e| {
        Error::new(ErrorKind::CantOpenPatch(file_name.clone(), e))
      })?;
      let content = unsafe { Mmap::map(&file)? };
      Ok(Self::File {
        name: file_name,
        content,
      })
    }))
  }

  pub fn name(&self) -> &str {
    match self {
      Self::Stdin(_) => "<stdin>",
      Self::File { name, .. } => name,
    }
  }

  pub fn content(&self) -> &[u8] {
    match self {
      Self::Stdin(c) => c,
      Self::File { content, .. } => content,
    }
  }
}
