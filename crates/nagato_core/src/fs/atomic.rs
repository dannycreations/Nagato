use std::{
  io::{
    BufWriter, Error as IoError, ErrorKind as IoErrorKind, Result as IoResult,
    Write,
  },
  path::{Path, PathBuf},
};

use anyhow::Context;
use tempfile::NamedTempFile;

use crate::{Error, ErrorKind};

/// Atomic file writer that uses a temporary file and renames it on commit.
/// This ensures that the destination file is only updated if the write succeeds.
pub struct AtomicWriter {
  writer: BufWriter<NamedTempFile>,
  dest_path: PathBuf,
}

impl AtomicWriter {
  /// Create a new atomic writer for the given path.
  /// The temporary file is created in the same directory as the destination file.
  pub fn new(path: &Path) -> Result<Self, Error> {
    let parent = path.parent().ok_or_else(|| {
      Error::new(ErrorKind::Io(IoError::new(
        IoErrorKind::InvalidInput,
        "Destination path has no parent directory",
      )))
    })?;
    let tempfile = NamedTempFile::new_in(parent)
      .with_context(|| format!("failed to create tempfile in {:?}", parent))?;
    // 1MB buffer for high-performance writes
    let writer = BufWriter::with_capacity(1024 * 1024, tempfile);

    Ok(Self {
      writer,
      dest_path: path.to_path_buf(),
    })
  }

  /// Commit the changes by persisting the temporary file to the destination path.
  pub fn commit(mut self) -> Result<(), Error> {
    self
      .writer
      .flush()
      .context("failed to flush atomic writer")?;
    self
      .writer
      .into_inner()
      .map_err(|e| e.into_error())
      .context("failed to get inner writer from BufWriter")?
      .persist(&self.dest_path)
      .context("failed to persist temporary file")?;
    Ok(())
  }
}

impl Write for AtomicWriter {
  #[inline]
  fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
    self.writer.write(buf)
  }

  #[inline]
  fn flush(&mut self) -> IoResult<()> {
    self.writer.flush()
  }
}
