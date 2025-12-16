use std::{
  fs::{self, File},
  io::{self, BufWriter, Write},
  path::{Path, PathBuf},
};

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
    let writer = BufWriter::with_capacity(128 * 1024, tempfile);

    Ok(Self {
      writer,
      dest_path: path.to_path_buf(),
    })
  }

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

impl OsFileSystem {
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self { root: root.into() }
  }

  fn resolve(&self, path: &[u8]) -> Result<PathBuf, Error> {
    use bstr::ByteSlice;
    path
      .to_path()
      .map(|p| self.root.join(p))
      .map_err(|_| Error {
        line: None,
        kind: ErrorKind::InvalidPath,
      })
  }

  fn resolve_mut(&self, path: &[u8]) -> Result<PathBuf, Error> {
    let p = self.resolve(path)?;
    if let Some(parent) = p.parent() {
      fs::create_dir_all(parent)?;
    }
    Ok(p)
  }
}

impl FileSystem for OsFileSystem {
  fn exists(&self, path: &[u8]) -> bool {
    self.resolve(path).is_ok_and(|p| p.exists())
  }

  fn read(&self, path: &[u8]) -> Result<Mmap, Error> {
    let file = File::open(self.resolve(path)?)?;
    unsafe { Mmap::map(&file) }.map_err(Into::into)
  }

  fn write(&mut self, path: &[u8]) -> Result<AtomicWriter, Error> {
    AtomicWriter::new(&self.resolve_mut(path)?).map_err(Into::into)
  }

  fn copy(&mut self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    fs::copy(self.resolve(from)?, self.resolve_mut(to)?)?;
    Ok(())
  }

  fn remove_file(&mut self, path: &[u8]) -> Result<(), Error> {
    fs::remove_file(self.resolve(path)?).map_err(Into::into)
  }

  fn rename(&mut self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    fs::rename(self.resolve(from)?, self.resolve_mut(to)?).map_err(Into::into)
  }

  fn set_permissions(&mut self, path: &[u8], _mode: u32) -> Result<(), Error> {
    let path = self.resolve(path)?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
      let _ = path;
    }
    Ok(())
  }
}
