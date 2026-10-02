#[cfg(unix)]
use std::fs::Permissions;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
  cell::RefCell,
  collections::{HashMap, HashSet},
  env,
  ffi::OsStr,
  fs::{self, remove_file, File},
  io::{Error as IoError, ErrorKind as IoErrorKind},
  path::{Component, Path, PathBuf},
};

use bstr::ByteSlice;
use memmap2::Mmap;
use tempfile::TempDir;

use crate::{
  utils::{to_path_buf, IsDevNull},
  AtomicWriter, Error, ErrorKind,
};

// Upper bound on cached path resolutions before the cache resets itself.
const PATH_CACHE_LIMIT: usize = 10_000;

/// Setuid and setgid bits, stripped so that a patch cannot turn a file
/// executable under a user it does not belong to.
#[cfg(unix)]
const PRIVILEGE_BITS: u32 = 0o6000;

#[derive(Debug)]
pub struct FileSystem {
  root: PathBuf,
  staging: Option<TempDir>,
  deleted: RefCell<HashSet<PathBuf>>,
  resolved: RefCell<HashMap<Box<[u8]>, PathBuf>>,
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
      staging,
      deleted: RefCell::new(HashSet::new()),
      resolved: RefCell::new(HashMap::new()),
    }
  }

  #[inline]
  fn is_check(&self) -> bool {
    self.staging.is_some()
  }

  #[inline]
  pub fn exists(&self, path: &[u8]) -> bool {
    let Ok(rel) = self.resolve_relative(path) else {
      return false;
    };

    !self.deleted.borrow().contains(&rel) && self.effective_path(&rel).exists()
  }

  #[inline]
  pub fn read(&self, path: &[u8]) -> Result<Mmap, Error> {
    let rel = self.resolve_relative(path)?;
    if self.deleted.borrow().contains(&rel) {
      return Err(ErrorKind::Io(IoError::from(IoErrorKind::NotFound)).into());
    }
    let full_path = self.effective_path(&rel);

    let file = File::open(full_path)?;
    // SAFETY: The mapping is read-only and lives no longer than the patch run
    // that requested it. Nagato never writes a file through its own mapping;
    // it stages every write through `AtomicWriter` and replaces the path by
    // rename instead. A concurrent external truncation could still fault,
    // which is the exposure every mmap-based reader accepts in exchange for
    // avoiding a full copy of the source file.
    unsafe { Mmap::map(&file) }.map_err(Into::into)
  }

  pub fn write(&self, path: &[u8]) -> Result<AtomicWriter, Error> {
    let rel = self.resolve_relative(path)?;
    self.deleted.borrow_mut().remove(&rel);
    let full = self.destination_path(&rel);

    create_parent_dir(&full)?;
    AtomicWriter::new(&full)
  }

  pub fn copy(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    let from_rel = self.resolve_relative(from)?;
    let to_rel = self.resolve_relative(to)?;

    let from_path = self.effective_path(&from_rel);

    self.deleted.borrow_mut().remove(&to_rel);

    let to_path = self.destination_path(&to_rel);

    create_parent_dir(&to_path)?;

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
      remove_file_missing_ok(staged)?;
    }

    if self.is_check() {
      return Ok(());
    }

    remove_file_missing_ok(self.root.join(rel))
  }

  pub fn rename(&self, from: &[u8], to: &[u8]) -> Result<(), Error> {
    let from_rel = self.resolve_relative(from)?;
    let to_rel = self.resolve_relative(to)?;

    // If we are in check mode, rename is simulated via copy and remove.
    if self.is_check() {
      self.copy(from, to)?;
      self.remove(from)?;
      return Ok(());
    }

    // Normal rename
    let from_full = self.root.join(&from_rel);
    let to_full = self.root.join(&to_rel);

    if from_full == to_full {
      return Ok(());
    }

    create_parent_dir(&to_full)?;
    fs::rename(from_full, to_full).map_err(Into::into)
  }

  #[allow(unused_variables)]
  pub fn set_permissions(&self, path: &[u8], mode: u32) -> Result<(), Error> {
    #[cfg(unix)]
    {
      let rel = self.resolve_relative(path)?;
      let full_path = self.effective_path(&rel);

      // A check run only chmod'es its own staged copies; the real target on
      // disk has to stay untouched.
      if self.is_check() && full_path == self.root.join(&rel) {
        return Ok(());
      }

      let sanitized_mode = mode & !PRIVILEGE_BITS;
      fs::set_permissions(full_path, Permissions::from_mode(sanitized_mode))?;
    }
    Ok(())
  }

  fn resolve_relative(&self, path: &[u8]) -> Result<PathBuf, Error> {
    if let Some(res) = self.resolved.borrow().get(path) {
      return Ok(res.clone());
    }

    let path_obj = to_path_buf(path)?;
    let mut rel = PathBuf::with_capacity(path_obj.as_os_str().len());

    for component in path_obj.components() {
      match component {
        Component::Normal(c) => {
          check_component(c)?;
          rel.push(c);
        }
        Component::CurDir => continue,
        _ => return Err(invalid_path()),
      }
    }

    let res = rel.clone();
    let mut cache = self.resolved.borrow_mut();
    if cache.len() >= PATH_CACHE_LIMIT {
      cache.clear();
    }
    cache.insert(Box::from(path), rel);
    Ok(res)
  }

  fn get_staged_path(&self, rel: &Path) -> Option<PathBuf> {
    self.staging.as_ref().map(|s| s.path().join(rel))
  }

  fn destination_path(&self, rel: &Path) -> PathBuf {
    self
      .get_staged_path(rel)
      .unwrap_or_else(|| self.root.join(rel))
  }

  fn effective_path(&self, rel: &Path) -> PathBuf {
    self
      .get_staged_path(rel)
      .filter(|p| p.exists())
      .unwrap_or_else(|| self.root.join(rel))
  }
}

fn create_parent_dir(path: &Path) -> Result<(), Error> {
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent)?;
  }
  Ok(())
}

fn remove_file_missing_ok(path: PathBuf) -> Result<(), Error> {
  remove_file(path).or_else(|e| match e.kind() {
    IoErrorKind::NotFound => Ok(()),
    _ => Err(e.into()),
  })
}

fn invalid_path() -> Error {
  Error::new(ErrorKind::InvalidPath)
}

fn check_component(component: &OsStr) -> Result<(), Error> {
  let bytes = component.to_str().ok_or_else(invalid_path)?.as_bytes();

  if matches!(bytes.last(), Some(b'.' | b' ')) {
    return Err(invalid_path());
  }

  if let Some(tilde_pos) = bytes.find_byte(b'~') {
    check_tilde_restriction(bytes, tilde_pos)?;
  }

  let base_len = bytes.find_byte(b'.').unwrap_or(bytes.len());
  if is_reserved_name(&bytes[..base_len]) {
    return Err(invalid_path());
  }
  Ok(())
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
