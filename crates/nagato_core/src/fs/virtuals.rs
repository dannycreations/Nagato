use std::{
  cell::RefCell,
  collections::{HashMap, HashSet},
  env,
  fs::{self, remove_file, File},
  io::{Error as IoError, ErrorKind as IoErrorKind},
  path::{Component, Path, PathBuf},
};

use bstr::ByteSlice;
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
  resolved: RefCell<HashMap<Box<[u8]>, PathBuf>>,
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
      resolved: RefCell::new(HashMap::new()),
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
    self.get_staged_path(&rel).filter(|p| p.exists()).is_some()
      || self.root.join(rel).exists()
  }

  #[inline]
  pub fn read(&self, path: &[u8]) -> Result<Mmap, Error> {
    let rel = self.resolve_relative(path)?;
    if self.deleted.borrow().contains(&rel) {
      return Err(ErrorKind::Io(IoError::from(IoErrorKind::NotFound)).into());
    }
    let full_path = self
      .get_staged_path(&rel)
      .filter(|p| p.exists())
      .unwrap_or_else(|| self.root.join(rel));

    let file = File::open(full_path)?;
    // SAFETY: Mmap is used for efficient reading.
    unsafe { Mmap::map(&file) }.map_err(Into::into)
  }

  pub fn write(&self, path: &[u8]) -> Result<AtomicWriter, Error> {
    let rel = self.resolve_relative(path)?;
    self.deleted.borrow_mut().remove(&rel);
    let full = if let Some(staged) = self.get_staged_path(&rel) {
      staged
    } else {
      self.root.join(&rel)
    };

    if let Some(parent) = full.parent() {
      fs::create_dir_all(parent)?;
    }
    AtomicWriter::new(&full)
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

    // If from and to are the same path (after resolution), this is a no-op.
    if from_full == to_full {
      return Ok(());
    }

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
    if let Some(res) = self.resolved.borrow().get(path) {
      return Ok(res.clone());
    }

    let path_obj =
      to_path_buf(path).map_err(|_| Error::new(ErrorKind::InvalidPath))?;

    let mut rel = PathBuf::with_capacity(path_obj.as_os_str().len());
    for component in path_obj.components() {
      match component {
        Component::Normal(c) => {
          let s = c.to_str().ok_or(Error::new(ErrorKind::InvalidPath))?;
          let bytes = s.as_bytes();

          if let Some(b'.' | b' ') = bytes.last() {
            return Err(Error::new(ErrorKind::InvalidPath));
          }

          if bytes.contains(&b'~') {
            let mut i = 0;
            while i < bytes.len() {
              if bytes[i] == b'~'
                && i + 1 < bytes.len()
                && bytes[i + 1].is_ascii_digit()
              {
                return Err(Error::new(ErrorKind::InvalidPath));
              }
              i += 1;
            }
          }

          let base_len = bytes.find_byte(b'.').unwrap_or(bytes.len());
          if is_reserved_name(&bytes[..base_len]) {
            return Err(Error::new(ErrorKind::InvalidPath));
          }
          rel.push(c);
        }
        Component::CurDir => {}
        _ => return Err(Error::new(ErrorKind::InvalidPath)),
      }
    }

    let res = rel.clone();
    self.resolved.borrow_mut().insert(Box::from(path), rel);
    Ok(res)
  }

  fn get_staged_path(&self, rel: &Path) -> Option<PathBuf> {
    self.staging.as_ref().map(|s| s.path().join(rel))
  }
}

fn is_reserved_name(bytes: &[u8]) -> bool {
  match bytes.len() {
    3 => {
      let b0 = bytes[0] | 0x20;
      let b1 = bytes[1] | 0x20;
      let b2 = bytes[2] | 0x20;
      matches!(
        (b0, b1, b2),
        (b'c', b'o', b'n')
          | (b'p', b'r', b'n')
          | (b'a', b'u', b'x')
          | (b'n', b'u', b'l')
      )
    }
    4 => {
      let b0 = bytes[0] | 0x20;
      let b1 = bytes[1] | 0x20;
      let b2 = bytes[2] | 0x20;
      let b3 = bytes[3];
      matches!((b0, b1, b2), (b'c', b'o', b'm') | (b'l', b'p', b't'))
        && b3.is_ascii_digit()
    }
    6 => bytes.eq_ignore_ascii_case(b"CLOCK$"),
    _ => false,
  }
}
