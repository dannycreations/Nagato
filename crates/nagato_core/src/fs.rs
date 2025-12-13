use std::{
  fs::{self, File},
  io::{self, BufWriter, Write},
  path::{Path, PathBuf},
};

use bstr::ByteSlice;
use memmap2::Mmap;
use tempfile::NamedTempFile;

pub struct AtomicWriter {
  writer: BufWriter<NamedTempFile>,
  dest_path: PathBuf,
}

impl AtomicWriter {
  pub fn new(path: &Path) -> io::Result<Self> {
    // Relying on `NamedTempFile::new_in` to handle directory creation simplifies
    // the logic and removes a potential point of failure. The parent directory
    // is guaranteed to exist before the temp file is created.
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tempfile = NamedTempFile::new_in(parent)?;
    let writer = BufWriter::new(tempfile);

    Ok(Self {
      writer,
      dest_path: path.to_path_buf(),
    })
  }

  pub fn commit(mut self) -> io::Result<()> {
    self.writer.flush()?;

    // Using `io::Error::other` provides a more descriptive error message
    // if the buffer writer is poisoned, which can help in debugging.
    let tempfile = self.writer.into_inner().map_err(|_| {
      io::Error::other("Buffer writer was poisoned, likely due to a panic")
    })?;

    tempfile.persist(&self.dest_path).map_err(|e| e.error)?;
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
  fn read(&self, path: &[u8]) -> io::Result<Mmap>;
  fn write(&mut self, path: &[u8]) -> io::Result<AtomicWriter>;
  fn copy(&mut self, from: &[u8], to: &[u8]) -> io::Result<()>;
  fn remove_file(&mut self, path: &[u8]) -> io::Result<()>;
  fn rename(&mut self, from: &[u8], to: &[u8]) -> io::Result<()>;
  fn set_permissions(&mut self, path: &[u8], mode: u32) -> io::Result<()>;
}

#[derive(Debug, Default)]
pub struct OsFileSystem {
  root: PathBuf,
}

impl OsFileSystem {
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self { root: root.into() }
  }

  fn absolute_path(&self, path: &[u8]) -> io::Result<PathBuf> {
    let path = path
      .to_path()
      .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    Ok(self.root.join(path))
  }

  fn ensure_parent_dir_exists(&self, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    Ok(())
  }
}

impl FileSystem for OsFileSystem {
  fn exists(&self, path: &[u8]) -> bool {
    self.absolute_path(path).is_ok_and(|p| p.exists())
  }

  fn read(&self, path: &[u8]) -> io::Result<Mmap> {
    let file = File::open(self.absolute_path(path)?)?;
    unsafe { Mmap::map(&file) }
  }

  fn write(&mut self, path: &[u8]) -> io::Result<AtomicWriter> {
    let abs_path = self.absolute_path(path)?;
    AtomicWriter::new(&abs_path)
  }

  fn copy(&mut self, from: &[u8], to: &[u8]) -> io::Result<()> {
    let from_abs = self.absolute_path(from)?;
    let to_abs = self.absolute_path(to)?;

    self.ensure_parent_dir_exists(&to_abs)?;

    fs::copy(from_abs, to_abs)?;
    Ok(())
  }

  fn remove_file(&mut self, path: &[u8]) -> io::Result<()> {
    fs::remove_file(self.absolute_path(path)?)
  }

  fn rename(&mut self, from: &[u8], to: &[u8]) -> io::Result<()> {
    let from_abs = self.absolute_path(from)?;
    let to_abs = self.absolute_path(to)?;

    self.ensure_parent_dir_exists(&to_abs)?;

    fs::rename(from_abs, to_abs)
  }

  fn set_permissions(&mut self, path: &[u8], mode: u32) -> io::Result<()> {
    let abs_path = self.absolute_path(path)?;
    #[cfg(unix)]
    {
      use std::{fs::Permissions, os::unix::fs::PermissionsExt};
      fs::set_permissions(abs_path, Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
      let _ = (abs_path, mode);
      Ok(())
    }
  }
}
