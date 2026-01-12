use std::{
  io::{
    BufWriter, Error as IoError, ErrorKind as IoErrorKind, Result as IoResult,
    Write,
  },
  path::{Path, PathBuf},
};

use tempfile::NamedTempFile;

use crate::{Error, ErrorKind};

const WRITE_BUFFER_SIZE: usize = 1024 * 1024;

pub struct AtomicWriter {
  writer: BufWriter<NamedTempFile>,
  dest_path: PathBuf,
}

impl AtomicWriter {
  pub fn new(path: &Path) -> Result<Self, Error> {
    let parent = path.parent().ok_or_else(|| {
      Error::new(ErrorKind::Io(IoError::new(
        IoErrorKind::InvalidInput,
        "Destination path has no parent directory",
      )))
    })?;
    let tempfile = NamedTempFile::new_in(parent)?;
    let writer = BufWriter::with_capacity(WRITE_BUFFER_SIZE, tempfile);

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
      .persist(&self.dest_path)
      .map_err(Error::from)?;
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
