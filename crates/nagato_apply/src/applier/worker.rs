use std::io::{self, sink};

use memmap2::Mmap;
use nagato_core::{
  error::{Error, ErrorKind},
  fs::FileSystem,
};

use crate::{applier::apply, models::patch::Patch};

/// Ignore I/O "Not Found" errors.
pub fn ignore_not_found(res: Result<(), Error>) -> Result<(), Error> {
  match res {
    Err(Error {
      kind: ErrorKind::Io(e),
      ..
    }) if e.kind() == io::ErrorKind::NotFound => Ok(()),
    res => res,
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

/// Handle file deletion by applying the patch to a sink and removing the file.
pub fn handle_file_deletion(
  fs: &FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  let source = read_source_or_empty(fs, source_path)?;
  let source_slice = source.as_deref().unwrap_or(&[]);
  apply(&mut sink(), patch, source_slice)?;

  ignore_not_found(fs.remove_file(source_path))?;
  Ok(())
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
  let source_path = patch.source_file();
  let mut writer = fs.write(patch.new_file)?;
  {
    let source = read_source_or_empty(fs, source_path)?;
    let source_slice = source.as_deref().unwrap_or(&[]);
    apply(&mut writer, patch, source_slice)?;
  }
  writer.commit()?;

  if patch.rename_to.is_some() && source_path != patch.new_file {
    ignore_not_found(fs.remove_file(source_path))?;
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

  match (patch.new_file, patch.hunks.is_empty()) {
    (b"/dev/null", _) => handle_file_deletion(fs, patch)?,
    (_, true) => handle_metadata_change(fs, patch)?,
    (_, false) => handle_content_change(fs, patch)?,
  }

  if patch.new_file != b"/dev/null" {
    if let Some(mode) = patch.new_mode.or(patch.index_mode) {
      fs.set_permissions(patch.new_file, mode)?;
    }
  }

  Ok(())
}
