use std::{
  cell::RefCell,
  collections::HashSet,
  env,
  fs::{self, remove_file, File},
  io::{Error as IoError, ErrorKind as IoErrorKind},
  path::{Component, Path, PathBuf},
};

use memmap2::Mmap;
use tempfile::TempDir;

use crate::{
  traits::IsDevNull, utils::to_path_buf, AtomicWriter, Error, ErrorKind,
};

#[derive(Debug)]
pub struct FileSystem {
  root: PathBuf,
  check: bool,
  staging: Option<TempDir>,
  deleted: RefCell<HashSet<PathBuf>>,
}

impl Default for FileSystem {
  fn default() -> Self {
    Self::new(
      env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
      false,
    )
  }
}

impl FileSystem {
  pub fn new(root: impl Into<PathBuf>, check: bool) -> Self {
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

    let staging = if check {
      Some(TempDir::new().expect("failed to create staging directory"))
    } else {
      None
    };

    Self {
      root,
      check,
      staging,
      deleted: RefCell::new(HashSet::new()),
    }
  }

  #[inline]
  pub fn exists(&self, path: &[u8]) -> bool {
    let rel = match self.resolve_relative(path) {
      Ok(p) => p,
      Err(_) => return false,
    };
    if self.deleted.borrow().contains(&rel) {
      return false;
    }
    if let Some(staged) = self.get_staged_path(&rel) {
      if staged.exists() {
        return true;
      }
    }
    self.root.join(rel).exists()
  }

  #[inline]
  pub fn read(&self, path: &[u8]) -> Result<Mmap, Error> {
    let rel = self.resolve_relative(path)?;
    if self.deleted.borrow().contains(&rel) {
      return Err(ErrorKind::Io(IoError::from(IoErrorKind::NotFound)).into());
    }
    let full_path = if let Some(staged) = self.get_staged_path(&rel) {
      if staged.exists() {
        staged
      } else {
        self.root.join(rel)
      }
    } else {
      self.root.join(rel)
    };
    let file = File::open(full_path)?;
    // SAFETY: Mmap is used for efficient reading.
    unsafe { Mmap::map(&file) }.map_err(Into::into)
  }

  pub fn write(&self, path: &[u8]) -> Result<AtomicWriter, Error> {
    let rel = self.resolve_relative(path)?;
    self.deleted.borrow_mut().remove(&rel);
    if let Some(staged) = self.get_staged_path(&rel) {
      if let Some(parent) = staged.parent() {
        fs::create_dir_all(parent)?;
      }
      AtomicWriter::new(&staged)
    } else {
      let full = self.root.join(&rel);
      if let Some(parent) = full.parent() {
        fs::create_dir_all(parent)?;
      }
      AtomicWriter::new(&full)
    }
  }

  pub fn copy(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    let from_rel = self.resolve_relative(from)?;
    let to_rel = self.resolve_relative(to)?;

    let from_path = if let Some(staged) = self.get_staged_path(&from_rel) {
      if staged.exists() {
        staged
      } else {
        self.root.join(&from_rel)
      }
    } else {
      self.root.join(&from_rel)
    };

    self.deleted.borrow_mut().remove(&to_rel);
    if let Some(to_staged) = self.get_staged_path(&to_rel) {
      if let Some(parent) = to_staged.parent() {
        fs::create_dir_all(parent)?;
      }
      fs::copy(from_path, to_staged)?;
    } else {
      let to_full = self.root.join(&to_rel);
      if let Some(parent) = to_full.parent() {
        fs::create_dir_all(parent)?;
      }
      fs::copy(from_path, to_full)?;
    }
    Ok(())
  }

  pub fn remove(&self, path: &[u8]) -> Result<(), Error> {
    if path.is_dev_null() {
      return Ok(());
    }
    let rel = self.resolve_relative(path)?;
    self.deleted.borrow_mut().insert(rel.clone());
    if let Some(staged) = self.get_staged_path(&rel) {
      if staged.exists() {
        let _ = remove_file(staged);
      }
    }
    if !self.check {
      let full = self.root.join(rel);
      if full.exists() {
        remove_file(full)?;
      }
    }
    Ok(())
  }

  pub fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    let from_rel = self.resolve_relative(from)?;
    let to_rel = self.resolve_relative(to)?;

    // If we are in check mode, rename is simulated via copy and remove.
    if self.check {
      self.copy(from, to)?;
      self.remove(from)?;
      return Ok(());
    }

    // Normal rename
    let from_full = self.root.join(&from_rel);
    let to_full = self.root.join(&to_rel);
    if let Some(parent) = to_full.parent() {
      fs::create_dir_all(parent)?;
    }
    fs::rename(from_full, to_full).map_err(Into::into)
  }

  #[allow(unused_variables)]
  pub fn set_permissions(&self, path: &[u8], mode: u32) -> Result<(), Error> {
    #[cfg(unix)]
    {
      let rel = self.resolve_relative(path)?;
      let full_path = if let Some(staged) = self.get_staged_path(&rel) {
        if staged.exists() {
          staged
        } else {
          self.root.join(rel)
        }
      } else {
        self.root.join(rel)
      };

      if !self.check
        || full_path.starts_with(self.staging.as_ref().unwrap().path())
      {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(full_path, fs::Permissions::from_mode(mode))?;
      }
    }
    Ok(())
  }

  fn resolve_relative(&self, path: &[u8]) -> Result<PathBuf, Error> {
    let path =
      to_path_buf(path).map_err(|_| Error::new(ErrorKind::InvalidPath))?;

    let mut rel = PathBuf::new();
    // Path resolution is restricted to normal components and current directory references within the root to prevent unauthorized directory traversal.
    for component in path.components() {
      match component {
        Component::Normal(c) => {
          let s = c.to_str().ok_or(Error::new(ErrorKind::InvalidPath))?;

          // Windows ignores trailing dots and spaces, which can lead to collisions or security bypasses.
          if s.ends_with('.') || s.ends_with(' ') {
            return Err(Error::new(ErrorKind::InvalidPath));
          }

          // Windows 8.3 short names (e.g., PROGRA~1) can be used to bypass filters.
          if let Some(tilde_idx) = s.find('~') {
            let suffix = &s[tilde_idx + 1..];
            if suffix.as_bytes().first().is_some_and(u8::is_ascii_digit) {
              return Err(Error::new(ErrorKind::InvalidPath));
            }
          }

          let base = s.split('.').next().unwrap_or(s);
          if is_reserved_name(base) {
            return Err(Error::new(ErrorKind::InvalidPath));
          }
          rel.push(c);
        }
        Component::CurDir => {}
        _ => return Err(Error::new(ErrorKind::InvalidPath)),
      }
    }
    Ok(rel)
  }

  fn get_staged_path(&self, rel: &Path) -> Option<PathBuf> {
    self.staging.as_ref().map(|s| s.path().join(rel))
  }
}

fn is_reserved_name(name: &str) -> bool {
  // Windows reserved device names are identified by performing case-insensitive matches using a pre-calculated bitmask for the first 3 characters to minimize string comparisons.
  let bytes = name.as_bytes();
  if bytes.len() < 3 || bytes.len() > 6 {
    return false;
  }

  let head = (bytes[0].to_ascii_uppercase() as u32) << 16
    | (bytes[1].to_ascii_uppercase() as u32) << 8
    | (bytes[2].to_ascii_uppercase() as u32);

  match bytes.len() {
    3 => matches!(head, 0x434F4E | 0x50524E | 0x415558 | 0x4E554C), // CON, PRN, AUX, NUL
    4 => {
      (head == 0x434F4D || head == 0x4C5054) && bytes[3].is_ascii_digit() // COM0-9, LPT0-9
    }
    6 => {
      // CLOCK$
      head == 0x434C4F
        && bytes[3].eq_ignore_ascii_case(&b'C')
        && bytes[4].eq_ignore_ascii_case(&b'K')
        && bytes[5] == b'$'
    }
    _ => false,
  }
}
