use std::{
  fs::{self, File},
  io::{self, BufWriter, Write},
  path::{Component, Path, PathBuf},
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
    // Increased buffer size to 1MB for better performance on large writes
    let writer = BufWriter::with_capacity(1024 * 1024, tempfile);

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
  #[inline]
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.writer.write(buf)
  }

  #[inline]
  fn flush(&mut self) -> io::Result<()> {
    self.writer.flush()
  }
}

#[derive(Debug, Default)]
pub struct FileSystem {
  root: PathBuf,
}

impl FileSystem {
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self { root: root.into() }
  }

  pub fn exists(&self, path: &[u8]) -> bool {
    self.resolve(path).is_ok_and(|p| p.exists())
  }

  pub fn read(&self, path: &[u8]) -> Result<Mmap, Error> {
    let file = File::open(self.resolve(path)?)?;
    // SAFETY: We map the file into memory. The file must not be concurrently modified.
    unsafe { Mmap::map(&file) }.map_err(Into::into)
  }

  pub fn write(&self, path: &[u8]) -> Result<AtomicWriter, Error> {
    AtomicWriter::new(&self.resolve_mut(path)?).map_err(Into::into)
  }

  pub fn copy(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    fs::copy(self.resolve(from)?, self.resolve_mut(to)?)?;
    Ok(())
  }

  pub fn remove_file(&self, path: &[u8]) -> Result<(), Error> {
    fs::remove_file(self.resolve(path)?).map_err(Into::into)
  }

  pub fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    fs::rename(self.resolve(from)?, self.resolve_mut(to)?).map_err(Into::into)
  }

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

  fn resolve(&self, path: &[u8]) -> Result<PathBuf, Error> {
    use bstr::ByteSlice;
    let path = path.to_path().map_err(|_| Error {
      line: None,
      kind: ErrorKind::InvalidPath,
    })?;

    // Pre-calculate capacity to avoid reallocations
    let mut dest = PathBuf::with_capacity(
      self.root.as_os_str().len() + path.as_os_str().len() + 1,
    );
    dest.push(&self.root);

    for component in path.components() {
      match component {
        Component::Normal(c) => dest.push(c),
        Component::CurDir => {}
        Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
          return Err(Error {
            line: None,
            kind: ErrorKind::InvalidPath,
          });
        }
      }
    }

    Ok(dest)
  }

  fn resolve_mut(&self, path: &[u8]) -> Result<PathBuf, Error> {
    let p = self.resolve(path)?;
    if let Some(parent) = p.parent() {
      fs::create_dir_all(parent)?;
    }
    Ok(p)
  }
}
