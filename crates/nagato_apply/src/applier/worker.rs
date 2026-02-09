use std::io::{sink, Write as IoWrite};

use memmap2::Mmap;
use nagato_core::{Error, ErrorKind, FileSystem, IgnoreNotFound, IsDevNull};

use crate::{apply, Patch};

pub fn read_source_or_empty(
  fs: &FileSystem,
  path: &[u8],
) -> Result<Option<Mmap>, Error> {
  if path.is_dev_null() {
    return Ok(None);
  }
  fs.read(path).map(Some).ignore_not_found()
}

fn apply_to_writer(
  fs: &FileSystem,
  patch: &Patch<'_>,
  writer: &mut (impl IoWrite + ?Sized),
) -> Result<(), Error> {
  let source = read_source_or_empty(fs, patch.source_file())?;
  apply(writer, patch, source.as_deref().unwrap_or(&[]))
}

fn ensure_not_exists(fs: &FileSystem, path: &[u8]) -> Result<(), Error> {
  if fs.exists(path) {
    Err(Error::new(ErrorKind::AlreadyExists))
  } else {
    Ok(())
  }
}

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

  // Patch application logic is dispatched based on the presence of content changes and the nature of the file operation to minimize redundant I/O.
  let result = if check || is_deletion {
    apply_to_writer(fs, patch, &mut sink())?;
    // File deletion is performed by directly invoking the removal operation on non-null source paths to ensure consistent state after patch application.
    if !check && is_deletion && !patch.source_file().is_dev_null() {
      fs.remove_file(patch.source_file())?;
    }
    Ok(())
  } else if has_content {
    if patch.old_file.is_dev_null() {
      ensure_not_exists(fs, &patch.new_file)?;
    }

    let mut writer = fs.write(&patch.new_file)?;
    apply_to_writer(fs, patch, &mut writer)?;
    writer.commit()?;

    let source_path = patch.source_file();
    if patch.rename_to.is_some() && patch.new_file != source_path {
      fs.remove_file(source_path).ignore_not_found()?;
    }
    Ok(())
  } else {
    let source_path = patch.source_file();
    // Structural changes like renames, copies, or file creations are handled by mapping the intended operation to the corresponding filesystem primitive.
    if patch.rename_to.is_some() {
      fs.rename(source_path, &patch.new_file)?;
    } else if patch.copy_to.is_some() {
      fs.copy(source_path, &patch.new_file)?;
    } else if patch.old_file.is_dev_null() {
      ensure_not_exists(fs, &patch.new_file)?;
      fs.write(&patch.new_file)?.commit()?;
    }
    Ok(())
  };

  result.map_err(|e: Error| {
    e.with_file(String::from_utf8_lossy(patch.filename()))
  })?;

  if !check && !patch.new_file.is_dev_null() {
    if let Some(mode) = patch.new_mode {
      fs.set_permissions(&patch.new_file, mode)?;
    }
  }

  Ok(())
}
