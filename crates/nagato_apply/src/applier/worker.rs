use std::io::{self, sink, ErrorKind as IoErrorKind};

use memmap2::Mmap;
use nagato_core::{Error, ErrorKind, FileSystem};

use crate::{apply, Patch};

/// Extension trait for Result to easily ignore "Not Found" I/O errors.
trait IgnoreNotFound {
  fn ignore_not_found(self) -> Self;
}

impl IgnoreNotFound for Result<(), Error> {
  fn ignore_not_found(self) -> Self {
    match self {
      Err(Error {
        kind: ErrorKind::Io(e),
        ..
      }) if e.kind() == IoErrorKind::NotFound => Ok(()),
      res => res,
    }
  }
}

/// Read source file into memory map, returning None if path is /dev/null or file not found.
pub fn read_source_or_empty(
  fs: &FileSystem,
  path: &[u8],
) -> Result<Option<Mmap>, Error> {
  if path == b"/dev/null" {
    return Ok(None);
  }
  match fs.read(path) {
    Ok(mmap) => Ok(Some(mmap)),
    Err(Error {
      kind: ErrorKind::Io(e),
      ..
    }) if e.kind() == io::ErrorKind::NotFound => Ok(None),
    Err(e) => Err(e),
  }
}

/// Helper to apply patch to a writer and handle source reading.
fn apply_to_writer(
  fs: &FileSystem,
  patch: &Patch<'_>,
  writer: &mut impl io::Write,
) -> Result<(), Error> {
  let source = read_source_or_empty(fs, patch.source_file())?;
  apply(writer, patch, source.as_deref().unwrap_or(&[]))
}

/// Handle file deletion by applying the patch to a sink and removing the file.
pub fn handle_file_deletion(
  fs: &FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  apply_to_writer(fs, patch, &mut sink())?;
  fs.remove_file(patch.source_file()).ignore_not_found()
}

/// Handle metadata-only changes (renames, copies, or creating empty files).
pub fn handle_metadata_change(
  fs: &FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  if patch.rename_to.is_some() {
    fs.rename(source_path, patch.new_file)?;
  } else if patch.copy_to.is_some() {
    fs.copy(source_path, patch.new_file)?;
  } else if patch.old_file == b"/dev/null" {
    fs.write(patch.new_file)?.commit()?;
  }
  Ok(())
}

/// Handle content changes by applying the patch to a new file.
pub fn handle_content_change(
  fs: &FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  let mut writer = fs.write(patch.new_file)?;
  apply_to_writer(fs, patch, &mut writer)?;
  writer.commit()?;

  let source_path = patch.source_file();
  if patch.rename_to.is_some() && source_path != patch.new_file {
    fs.remove_file(source_path).ignore_not_found()?;
  }
  Ok(())
}

/// The main worker for applying a patch to the file system.
pub fn patch_file_worker(
  fs: &FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  if patch.binary && !patch.hunks.is_empty() {
    return Err(Error::new(ErrorKind::UnsupportedBinaryPatch));
  }

  if !patch.binary_fragments.is_empty() {
    return handle_content_change(fs, patch);
  }

  if patch.new_file == b"/dev/null" {
    handle_file_deletion(fs, patch)?;
  } else if patch.hunks.is_empty() {
    handle_metadata_change(fs, patch)?;
  } else {
    handle_content_change(fs, patch)?;
  }

  if patch.new_file != b"/dev/null" {
    if let Some(mode) = patch.new_mode.or(patch.index_mode) {
      fs.set_permissions(patch.new_file, mode)?;
    }
  }

  Ok(())
}
