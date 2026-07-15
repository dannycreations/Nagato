#[cfg(unix)]
use std::fs::Permissions;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
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

    let staging = check
      .then(|| TempDir::new().expect("failed to create staging directory"));

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
      Ok(r) => r,
      Err(_) => return false,
    };

    if self.deleted.borrow().contains(&rel) {
      return false;
    }

    let staged_path = self.get_staged_path(&rel);
    if let Some(staged) = staged_path {
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
    let full = match self.get_staged_path(&rel) {
      Some(staged) => staged,
      None => self.root.join(&rel),
    };

    if let Some(parent) = full.parent() {
      fs::create_dir_all(parent)?;
    }
    AtomicWriter::new(&full)
  }

  pub fn copy(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    let from_rel = self.resolve_relative(from)?;
    let to_rel = self.resolve_relative(to)?;

    let from_path = match self.get_staged_path(&from_rel) {
      Some(staged) if staged.exists() => staged,
      _ => self.root.join(&from_rel),
    };

    self.deleted.borrow_mut().remove(&to_rel);

    let to_path = match self.get_staged_path(&to_rel) {
      Some(staged) => staged,
      None => self.root.join(&to_rel),
    };

    if let Some(parent) = to_path.parent() {
      fs::create_dir_all(parent)?;
    }

    fs::copy(from_path, to_path)?;
    Ok(())
  }

  pub fn remove(&self, path: &[u8]) -> Result<(), Error> {
    if path.is_dev_null() {
      return Ok(());
    }
    let rel = self.resolve_relative(path)?;
    self.deleted.borrow_mut().insert(rel.clone());

    if let Some(staged) = self.get_staged_path(&rel) {
      match remove_file(staged) {
        Ok(_) => {}
        Err(e) if e.kind() == IoErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
      }
    }

    if self.check {
      return Ok(());
    }

    let full = self.root.join(rel);
    match remove_file(full) {
      Ok(_) => Ok(()),
      Err(e) if e.kind() == IoErrorKind::NotFound => Ok(()),
      Err(e) => Err(e.into()),
    }
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
      let full_path = match self.get_staged_path(&rel) {
        Some(staged) if staged.exists() => staged,
        _ => self.root.join(&rel),
      };

      let is_staged = self
        .staging
        .as_ref()
        .map(|s| full_path.starts_with(s.path()))
        .unwrap_or(false);

      if !self.check || is_staged {
        let sanitized_mode = mode & !0o6000;
        fs::set_permissions(full_path, Permissions::from_mode(sanitized_mode))?;
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
      let c = match component {
        Component::Normal(c) => c,
        Component::CurDir => continue,
        _ => return Err(Error::new(ErrorKind::InvalidPath)),
      };

      let s = c.to_str().ok_or(Error::new(ErrorKind::InvalidPath))?;
      let bytes = s.as_bytes();

      if matches!(bytes.last(), Some(b'.' | b' ')) {
        return Err(Error::new(ErrorKind::InvalidPath));
      }

      if let Some(tilde_pos) = bytes.find_byte(b'~') {
        check_tilde_restriction(bytes, tilde_pos)?;
      }

      let base_len = bytes.find_byte(b'.').unwrap_or(bytes.len());
      if is_reserved_name(&bytes[..base_len]) {
        return Err(Error::new(ErrorKind::InvalidPath));
      }
      rel.push(c);
    }

    let res = rel.clone();
    let mut cache = self.resolved.borrow_mut();
    if cache.len() >= 10_000 {
      cache.clear();
    }
    cache.insert(Box::from(path), rel);
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

fn check_tilde_restriction(
  bytes: &[u8],
  mut tilde_pos: usize,
) -> Result<(), Error> {
  while tilde_pos + 1 < bytes.len() {
    if bytes[tilde_pos + 1].is_ascii_digit() {
      return Err(Error::new(ErrorKind::InvalidPath));
    }

    let Some(next_tilde) = bytes[tilde_pos + 1..].find_byte(b'~') else {
      break;
    };

    tilde_pos += 1 + next_tilde;
  }
  Ok(())
}
