use std::{
  env, fs,
  fs::File,
  path::{Component, PathBuf},
};

use bstr::ByteSlice;
use memmap2::Mmap;

use crate::{AtomicWriter, Error, ErrorKind};

#[derive(Debug, Default)]
pub struct FileSystem {
  root: PathBuf,
}

impl FileSystem {
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

  #[inline]
  pub fn exists(&self, path: &[u8]) -> bool {
    self.resolve(path).is_ok_and(|p| p.exists())
  }

  #[inline]
  pub fn read(&self, path: &[u8]) -> Result<Mmap, Error> {
    let path = self.resolve(path)?;
    let file = File::open(path)?;
    // SAFETY: Mmap is used for efficient reading.
    unsafe { Mmap::map(&file) }.map_err(Into::into)
  }

  pub fn write(&self, path: &[u8]) -> Result<AtomicWriter, Error> {
    AtomicWriter::new(&self.resolve_mut(path)?)
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
    let path = path
      .to_path()
      .map_err(|_| Error::new(ErrorKind::InvalidPath))?;

    let mut dest = self.root.clone();
    // Path resolution is restricted to normal components and current directory references within the root to prevent unauthorized directory traversal.
    for component in path.components() {
      match component {
        Component::Normal(c) => {
          if let Some(s) = c.to_str() {
            let base = s.split('.').next().unwrap_or(s);
            if is_reserved_name(base) {
              return Err(Error::new(ErrorKind::InvalidPath));
            }
          }
          dest.push(c);
        }
        Component::CurDir => {}
        _ => return Err(Error::new(ErrorKind::InvalidPath)),
      }
    }
    Ok(dest)
  }

  fn resolve_mut(&self, path: &[u8]) -> Result<PathBuf, Error> {
    let p = self.resolve(path)?;
    if let Some(parent) = p.parent() {
      // Ensure all parent directories exist before attempting to write.
      fs::create_dir_all(parent)?;
    }
    Ok(p)
  }
}

fn is_reserved_name(name: &str) -> bool {
  // Windows reserved device names are identified by performing case-insensitive matches against known three-character base names and validating trailing digits for COM and LPT devices.
  let bytes = name.as_bytes();
  match bytes.len() {
    3 => {
      bytes.eq_ignore_ascii_case(b"CON")
        || bytes.eq_ignore_ascii_case(b"PRN")
        || bytes.eq_ignore_ascii_case(b"AUX")
        || bytes.eq_ignore_ascii_case(b"NUL")
    }
    4 => {
      (bytes[..3].eq_ignore_ascii_case(b"COM")
        || bytes[..3].eq_ignore_ascii_case(b"LPT"))
        && bytes[3].is_ascii_digit()
    }
    _ => false,
  }
}
