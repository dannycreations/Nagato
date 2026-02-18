use std::{
  ffi::OsString,
  fs::File,
  io::{stdin, Read},
  iter::once,
};

use memmap2::Mmap;
use nagato_core::{Error, ErrorKind};

pub enum PatchSource {
  Stdin(Vec<u8>),
  File { name: Box<str>, content: Mmap },
}

impl PatchSource {
  pub fn iter(
    files: Vec<OsString>,
  ) -> Box<dyn Iterator<Item = Result<Self, Error>>> {
    if files.is_empty() {
      let mut content = Vec::new();
      // Standard input is read to completion when no files are specified, ensuring that piped patch data is fully captured before processing begins.
      let res = stdin()
        .read_to_end(&mut content)
        .map(|_| Self::Stdin(content))
        .map_err(Error::from);
      return Box::new(once(res));
    }

    // Multiple patch files are processed by mapping each path to a memory-mapped file source, providing efficient read access for the parser.
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
