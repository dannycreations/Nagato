use std::{
  env,
  fs::{self, File},
  path::{Component, PathBuf},
};

use bstr::ByteSlice;
use memmap2::Mmap;

use crate::{AtomicWriter, Error, ErrorKind};

/// A virtualized file system abstraction.
/// Ensures all file operations are relative to a root directory and prevents path traversal.
#[derive(Debug, Default)]
pub struct FileSystem {
  root: PathBuf,
}

impl FileSystem {
  /// Create a new file system rooted at the given path.
  pub fn new(root: impl Into<PathBuf>) -> Self {
    let root = root.into();
    // We try to make the root absolute to ensure consistent behavior.
    let root = if root.is_absolute() {
      root
    } else {
      env::current_dir()
        .map(|cwd| cwd.join(&root))
        .unwrap_or(root)
    };
    Self { root }
  }

  /// Check if a file exists at the given relative path.
  pub fn exists(&self, path: &[u8]) -> bool {
    self.resolve(path).is_ok_and(|p| p.exists())
  }

  /// Read a file into memory using a memory map.
  /// Handles the case where the file does not exist by returning an appropriate error.
  pub fn read(&self, path: &[u8]) -> Result<Mmap, Error> {
    let path = self.resolve(path)?;
    let file = File::open(path)?;
    // SAFETY: Mmap is used for efficient reading.
    unsafe { Mmap::map(&file) }.map_err(Into::into)
  }

  /// Create an atomic writer for the given relative path.
  pub fn write(&self, path: &[u8]) -> Result<AtomicWriter, Error> {
    AtomicWriter::new(&self.resolve_mut(path)?)
  }

  /// Copy a file from one relative path to another.
  pub fn copy(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    fs::copy(self.resolve(from)?, self.resolve_mut(to)?)?;
    Ok(())
  }

  /// Remove a file at the given relative path.
  pub fn remove_file(&self, path: &[u8]) -> Result<(), Error> {
    fs::remove_file(self.resolve(path)?).map_err(Into::into)
  }

  /// Rename a file from one relative path to another.
  pub fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    fs::rename(self.resolve(from)?, self.resolve_mut(to)?).map_err(Into::into)
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
  /// Standardizes path normalization by ignoring current directory components
  /// and rejecting any parent directory or root components to ensure safety.
  fn resolve(&self, path: &[u8]) -> Result<PathBuf, Error> {
    let path = path
      .to_path()
      .map_err(|_| Error::new(ErrorKind::InvalidPath))?;

    let mut dest = self.root.clone();
    let mut components = path.components().peekable();

    // If the path starts with a root or prefix, we skip it to treat the path as relative.
    // This is safer than rejecting it, as some patches might use absolute-looking paths
    // that should still be relative to the project root.
    while let Some(c) = components.peek() {
      match c {
        Component::RootDir | Component::Prefix(_) => {
          components.next();
        }
        _ => break,
      }
    }

    for component in components {
      match component {
        Component::Normal(c) => dest.push(c),
        Component::CurDir => {}
        _ => return Err(Error::new(ErrorKind::InvalidPath)),
      }
    }
    Ok(dest)
  }

  /// Resolve a relative byte path and ensure the parent directory exists.
  /// This is used for write operations where the directory structure might not exist yet.
  fn resolve_mut(&self, path: &[u8]) -> Result<PathBuf, Error> {
    let p = self.resolve(path)?;
    if let Some(parent) = p.parent() {
      // Ensure all parent directories exist before attempting to write.
      fs::create_dir_all(parent)?;
    }
    Ok(p)
  }
}
