use std::{
  fs::{self, File},
  io::{self, BufWriter, Write},
  path::{Component, Path, PathBuf},
};

use memmap2::Mmap;
use tempfile::NamedTempFile;

use crate::error::{Error, ErrorKind};

/// Atomic file writer that uses a temporary file and renames it on commit.
pub struct AtomicWriter {
  writer: BufWriter<NamedTempFile>,
  dest_path: PathBuf,
}

impl AtomicWriter {
  /// Create a new atomic writer for the given path.
  pub fn new(path: &Path) -> io::Result<Self> {
    let parent = path.parent().ok_or_else(|| {
      io::Error::new(
        io::ErrorKind::InvalidInput,
        "Destination path has no parent directory",
      )
    })?;
    let tempfile = NamedTempFile::new_in(parent)?;
    // 1MB buffer for high-performance writes
    let writer = BufWriter::with_capacity(1024 * 1024, tempfile);

    Ok(Self {
      writer,
      dest_path: path.to_path_buf(),
    })
  }

  /// Commit the changes by persisting the temporary file to the destination path.
  pub fn commit(mut self) -> Result<(), Error> {
    self.writer.flush()?;
    self
      .writer
      .into_inner()
      .map_err(|e| e.into_error())?
      .persist(&self.dest_path)?;
    Ok(())
  }
}

impl Write for AtomicWriter {
  #[inline]
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.writer.write(buf)
  }

  #[inline]
  fn flush(&mut self) -> io::Result<()> {
    self.writer.flush()
  }
}

/// A virtualized file system abstraction.
#[derive(Debug, Default)]
pub struct FileSystem {
  root: PathBuf,
}

impl FileSystem {
  /// Create a new file system rooted at the given path.
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self { root: root.into() }
  }

  /// Check if a file exists at the given relative path.
  pub fn exists(&self, path: &[u8]) -> bool {
    self.resolve(path).is_ok_and(|p| p.exists())
  }

  /// Read a file into memory using a memory map.
  pub fn read(&self, path: &[u8]) -> Result<Mmap, Error> {
    let path = self.resolve(path)?;
    let file = File::open(path)?;
    // SAFETY: Mmap is used for efficient reading.
    unsafe { Mmap::map(&file) }.map_err(Error::from)
  }

  /// Create an atomic writer for the given relative path.
  pub fn write(&self, path: &[u8]) -> Result<AtomicWriter, Error> {
    AtomicWriter::new(&self.resolve_mut(path)?).map_err(Error::from)
  }

  /// Copy a file from one relative path to another.
  pub fn copy(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    fs::copy(self.resolve(from)?, self.resolve_mut(to)?)?;
    Ok(())
  }

  /// Remove a file at the given relative path.
  pub fn remove_file(&self, path: &[u8]) -> Result<(), Error> {
    fs::remove_file(self.resolve(path)?).map_err(Error::from)
  }

  /// Rename a file from one relative path to another.
  pub fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    fs::rename(self.resolve(from)?, self.resolve_mut(to)?).map_err(Error::from)
  }

  /// Set file permissions.
  #[allow(unused_variables)]
  pub fn set_permissions(&self, path: &[u8], mode: u32) -> Result<(), Error> {
    #[cfg(unix)]
    {
      let path = self.resolve(path)?;
      use std::os::unix::fs::PermissionsExt;
      fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
  }

  /// Resolve a relative byte path to an absolute PathBuf, preventing traversal.
  fn resolve(&self, path: &[u8]) -> Result<PathBuf, Error> {
    use bstr::ByteSlice;
    let path = path
      .to_path()
      .map_err(|_| Error::new(ErrorKind::InvalidPath))?;

    let mut dest = PathBuf::with_capacity(
      self.root.as_os_str().len() + path.as_os_str().len() + 1,
    );
    dest.push(&self.root);

    for component in path.components() {
      match component {
        Component::Normal(c) => dest.push(c),
        Component::CurDir => {}
        _ => return Err(Error::new(ErrorKind::InvalidPath)),
      }
    }

    Ok(dest)
  }

  /// Resolve a relative byte path and ensure the parent directory exists.
  fn resolve_mut(&self, path: &[u8]) -> Result<PathBuf, Error> {
    let p = self.resolve(path)?;
    if let Some(parent) = p.parent() {
      fs::create_dir_all(parent)?;
    }
    Ok(p)
  }
}
