use std::io::{sink, Write as IoWrite};

use memmap2::Mmap;
use nagato_core::{Error, ErrorKind, FileSystem, IgnoreNotFound, IsDevNull};

use crate::{apply, Patch};

/// Read source file into memory map, returning None if path is /dev/null or file not found.
pub fn read_source_or_empty(
  fs: &FileSystem,
  path: &[u8],
) -> Result<Option<Mmap>, Error> {
  if path.is_dev_null() {
    return Ok(None);
  }
  fs.read(path).map(Some).ignore_not_found()
}

/// Helper to apply patch to a writer and handle source reading.
fn apply_to_writer(
  fs: &FileSystem,
  patch: &Patch<'_>,
  writer: &mut (impl IoWrite + ?Sized),
) -> Result<(), Error> {
  let source = read_source_or_empty(fs, patch.source_file())?;
  apply(writer, patch, source.as_deref().unwrap_or(&[]))
}

/// Handle metadata-only changes (renames, copies, or creating empty files).
fn handle_metadata_change(
  fs: &FileSystem,
  patch: &Patch<'_>,
  check: bool,
) -> Result<(), Error> {
  if check {
    return Ok(());
  }
  let source_path = patch.source_file();
  if patch.rename_to.is_some() {
    fs.rename(source_path, patch.new_file)?;
  } else if patch.copy_to.is_some() {
    fs.copy(source_path, patch.new_file)?;
  } else if patch.old_file.is_dev_null() {
    fs.write(patch.new_file)?.commit()?;
  }
  Ok(())
}

/// Handle content changes (including deletions) by applying the patch.
fn handle_application(
  fs: &FileSystem,
  patch: &Patch<'_>,
  check: bool,
) -> Result<(), Error> {
  let is_deletion = patch.new_file.is_dev_null();
  if check || is_deletion {
    apply_to_writer(fs, patch, &mut sink())?;
    if !check && is_deletion {
      fs.remove_file(patch.source_file()).ignore_not_found()?;
    }
    Ok(())
  } else {
    let mut writer = fs.write(patch.new_file)?;
    apply_to_writer(fs, patch, &mut writer)?;
    writer.commit()?;

    let source_path = patch.source_file();
    if patch.rename_to.is_some() && source_path != patch.new_file {
      fs.remove_file(source_path).ignore_not_found()?;
    }
    Ok(())
  }
}

/// The main worker for applying a patch to the file system.
pub fn patch_file_worker(
  fs: &FileSystem,
  patch: &Patch<'_>,
  check: bool,
) -> Result<(), Error> {
  if patch.binary && !patch.hunks.is_empty() {
    return Err(Error::new(ErrorKind::UnsupportedBinaryPatch));
  }

  let is_deletion = patch.new_file.is_dev_null();
  let result = if is_deletion || patch.has_content_changes() {
    handle_application(fs, patch, check)
  } else {
    handle_metadata_change(fs, patch, check)
  };

  result.map_err(|e| {
    e.with_file(String::from_utf8_lossy(patch.new_file).into_owned())
  })?;

  if !check && !patch.new_file.is_dev_null() {
    if let Some(mode) = patch.new_mode.or(patch.index_mode) {
      fs.set_permissions(patch.new_file, mode)?;
    }
  }

  Ok(())
}
