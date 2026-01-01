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
  File { name: String, content: Mmap },
}

impl PatchSource {
  /// Returns an iterator over patch sources from the provided list of files, or from stdin if empty.
  /// This allows processing patches one by one without loading all of them into memory at once.
  pub fn iter(
    files: Vec<OsString>,
  ) -> impl Iterator<Item = Result<Self, Error>> {
    let stdin_iter = if files.is_empty() {
      let mut content = Vec::new();
      let res = stdin()
        .read_to_end(&mut content)
        .map(|_| Self::Stdin(content))
        .map_err(Error::from);
      Some(std::iter::once(res))
    } else {
      None
    };

    let file_iter = files.into_iter().map(|path| {
      let name = path.to_string_lossy().to_string();
      let file = File::open(&path)
        .map_err(|e| Error::new(ErrorKind::CantOpenPatch(name.clone(), e)))?;
      let content = unsafe { Mmap::map(&file)? };
      Ok(Self::File { name, content })
    });

    stdin_iter.into_iter().flatten().chain(file_iter)
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
