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

/// Ensure the destination file does not exist when creating a new file.
fn ensure_not_exists(fs: &FileSystem, path: &[u8]) -> Result<(), Error> {
  if fs.exists(path) {
    Err(Error::new(ErrorKind::Io(std::io::Error::new(
      std::io::ErrorKind::AlreadyExists,
      "Destination file already exists",
    ))))
  } else {
    Ok(())
  }
}

/// The main worker for applying a patch to the file system.
/// Handles content changes, metadata updates, and deletions in a unified pipeline.
pub fn patch_file_worker(
  fs: &FileSystem,
  patch: &Patch<'_>,
  check: bool,
) -> Result<(), Error> {
  if patch.binary && !patch.hunks.is_empty() {
    return Err(Error::new(ErrorKind::UnsupportedBinaryPatch));
  }

  let is_deletion = patch.new_file.is_dev_null();
  let has_content = patch.has_content_changes();

  let result = if check || is_deletion {
    // Dry-run for checks, or full application to sink for deletions.
    apply_to_writer(fs, patch, &mut sink())?;
    if !check && is_deletion {
      let source_path = patch.source_file();
      if !source_path.is_dev_null() && !fs.exists(source_path) {
        return Err(Error::new(ErrorKind::Io(std::io::Error::new(
          std::io::ErrorKind::NotFound,
          "Source file to delete not found",
        ))));
      }
      fs.remove_file(source_path).ignore_not_found()?;
    }
    Ok(())
  } else if has_content {
    // Atomic content application.
    if patch.old_file.is_dev_null() {
      ensure_not_exists(fs, patch.new_file)?;
    }

    let mut writer = fs.write(patch.new_file)?;
    apply_to_writer(fs, patch, &mut writer)?;
    writer.commit()?;

    let source_path = patch.source_file();
    if patch.rename_to.is_some() && source_path != patch.new_file {
      fs.remove_file(source_path).ignore_not_found()?;
    }
    Ok(())
  } else {
    // Metadata-only changes (rename, copy, or create empty).
    let source_path = patch.source_file();
    if patch.rename_to.is_some() {
      fs.rename(source_path, patch.new_file)?;
    } else if patch.copy_to.is_some() {
      fs.copy(source_path, patch.new_file)?;
    } else if patch.old_file.is_dev_null() {
      ensure_not_exists(fs, patch.new_file)?;
      fs.write(patch.new_file)?.commit()?;
    }
    Ok(())
  };

  result.map_err(|e: Error| {
    e.with_file(String::from_utf8_lossy(patch.filename()).into_owned())
  })?;

  if !check && !patch.new_file.is_dev_null() {
    if let Some(mode) = patch.new_mode {
      fs.set_permissions(patch.new_file, mode)?;
    }
  }

  Ok(())
}
