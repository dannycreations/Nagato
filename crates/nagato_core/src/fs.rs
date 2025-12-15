use std::{
  fs::{self, File},
  io::{self, BufWriter, Write},
  path::{Path, PathBuf},
};

use bstr::ByteSlice;
use memmap2::Mmap;
use tempfile::NamedTempFile;

use crate::error::{Error, ErrorKind};

pub struct AtomicWriter {
  writer: BufWriter<NamedTempFile>,
  dest_path: PathBuf,
}

impl AtomicWriter {
  pub fn new(path: &Path) -> io::Result<Self> {
    let parent = path.parent().ok_or_else(|| {
      io::Error::new(
        io::ErrorKind::InvalidInput,
        "Destination path has no parent directory",
      )
    })?;
    let tempfile = NamedTempFile::new_in(parent)?;
    let writer = BufWriter::new(tempfile);

    Ok(Self {
      writer,
      dest_path: path.to_path_buf(),
    })
  }

  pub fn commit(mut self) -> Result<(), Error> {
    self.writer.flush()?;

    // The `into_inner` method can fail, and its error type doesn't automatically
    // convert to our custom `Error`. We now explicitly map it to a standard
    // `io::Error`, which allows the `?` operator to work correctly.
    let tempfile = self.writer.into_inner().map_err(|e| e.into_error())?;
    tempfile.persist(&self.dest_path)?;
    Ok(())
  }
}

impl io::Write for AtomicWriter {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.writer.write(buf)
  }

  fn flush(&mut self) -> io::Result<()> {
    self.writer.flush()
  }
}

pub trait FileSystem {
  fn exists(&self, path: &[u8]) -> bool;
  fn read(&self, path: &[u8]) -> Result<Mmap, Error>;
  fn write(&mut self, path: &[u8]) -> Result<AtomicWriter, Error>;
  fn copy(&mut self, from: &[u8], to: &[u8]) -> Result<(), Error>;
  fn remove_file(&mut self, path: &[u8]) -> Result<(), Error>;
  fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Error>;
  fn set_permissions(&mut self, path: &[u8], mode: u32) -> Result<(), Error>;
}

#[derive(Debug, Default)]
pub struct OsFileSystem {
  root: PathBuf,
}

// This function is being moved out of the `OsFileSystem` implementation because
// it doesn't depend on the struct's state (`self`). Making it a free function
// clarifies its role as a general utility for file system operations.
fn ensure_parent_dir_exists(path: &Path) -> io::Result<()> {
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent)?;
  }
  Ok(())
}

impl OsFileSystem {
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self { root: root.into() }
  }

  // The path conversion logic is now centralized here. It attempts to convert
  // a byte slice to a path, returning a specific `InvalidPath` error on failure.
  // This is more efficient than the previous `io::Error::other` approach.
  fn absolute_path(&self, path: &[u8]) -> Result<PathBuf, Error> {
    path
      .to_path()
      .map(|p| self.root.join(p))
      .map_err(|_| Error {
        line: None,
        kind: ErrorKind::InvalidPath,
      })
  }

  fn prepare_destination_path(&self, path: &[u8]) -> Result<PathBuf, Error> {
    let abs_path = self.absolute_path(path)?;
    ensure_parent_dir_exists(&abs_path)?;
    Ok(abs_path)
  }
}

impl FileSystem for OsFileSystem {
  fn exists(&self, path: &[u8]) -> bool {
    self.absolute_path(path).is_ok_and(|p| p.exists())
  }

  fn read(&self, path: &[u8]) -> Result<Mmap, Error> {
    let file = File::open(self.absolute_path(path)?)?;
    unsafe { Mmap::map(&file) }.map_err(Into::into)
  }

  fn write(&mut self, path: &[u8]) -> Result<AtomicWriter, Error> {
    let abs_path = self.prepare_destination_path(path)?;
    AtomicWriter::new(&abs_path).map_err(Into::into)
  }

  fn copy(&mut self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    let from_abs = self.absolute_path(from)?;
    let to_abs = self.prepare_destination_path(to)?;
    fs::copy(from_abs, to_abs)?;
    Ok(())
  }

  fn remove_file(&mut self, path: &[u8]) -> Result<(), Error> {
    fs::remove_file(self.absolute_path(path)?).map_err(Into::into)
  }

  fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    let from_abs = self.absolute_path(from)?;
    let to_abs = self.prepare_destination_path(to)?;
    fs::rename(from_abs, to_abs).map_err(Into::into)
  }

  fn set_permissions(&mut self, path: &[u8], mode: u32) -> Result<(), Error> {
    let abs_path = self.absolute_path(path)?;
    #[cfg(unix)]
    {
      use std::{fs::Permissions, os::unix::fs::PermissionsExt};
      fs::set_permissions(abs_path, Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
      let _ = (abs_path, mode);
    }
    Ok(())
  }
}
