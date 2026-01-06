use std::{
  env, fs,
  fs::File,
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
    // canonicalize() is avoided here because it requires the path to exist.
    let root = if root.is_absolute() {
      root
    } else {
      env::current_dir()
        .map(|cwd| cwd.join(&root))
        .unwrap_or_else(|_| root)
    };
    Self { root }
  }

  /// Check if a file exists at the given relative path.
  #[inline]
  pub fn exists(&self, path: &[u8]) -> bool {
    self.resolve(path).is_ok_and(|p| p.exists())
  }

  /// Read a file into memory using a memory map.
  /// Handles the case where the file does not exist by returning an appropriate error.
  #[inline]
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
    for component in path.components() {
      match component {
        // Normal components are appended to the root.
        Component::Normal(c) => {
          // Check for Windows reserved names (CON, PRN, AUX, NUL, COM1-9, LPT1-9).
          // These are problematic on Windows and should be avoided for cross-platform safety.
          if let Some(s) = c.to_str() {
            let base = s.split('.').next().unwrap_or(s);
            if is_reserved_name(base) {
              return Err(Error::new(ErrorKind::InvalidPath));
            }
          }
          dest.push(c);
        }
        // Current directory components are safely ignored.
        Component::CurDir => {}
        // Absolute paths, prefixes, and parent directory traversals are strictly forbidden
        // to ensure all operations remain within the designated root.
        Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
          return Err(Error::new(ErrorKind::InvalidPath))
        }
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

/// Check if a name is a Windows reserved device name.
/// See: https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file
fn is_reserved_name(name: &str) -> bool {
  let len = name.len();
  if len == 3 {
    return matches!(
      name.to_ascii_uppercase().as_str(),
      "CON" | "PRN" | "AUX" | "NUL"
    );
  }
  if len == 4 {
    let (prefix, digit) = name.split_at(3);
    return matches!(prefix.to_ascii_uppercase().as_str(), "COM" | "LPT")
      && digit.as_bytes()[0].is_ascii_digit();
  }
  false
}
