use std::io::sink;

use nagato_core::{Error, ErrorKind, FileSystem, IgnoreNotFound, IsDevNull};

use crate::{apply, Patch};

pub fn read_source_mapped(
  fs: &FileSystem,
  path: &[u8],
) -> Result<Option<memmap2::Mmap>, Error> {
  if path.is_dev_null() {
    return Ok(None);
  }
  fs.read(path).map(Some).ignore_not_found()
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
) -> Result<(), Error> {
  if patch.binary && !patch.hunks.is_empty() {
    return Err(Error::new(ErrorKind::UnsupportedBinaryPatch));
  }

  let is_deletion = patch.new_file.is_dev_null();
  let has_content = patch.has_content_changes();
  let source_path = patch.source_file();

  // Patch application logic is dispatched based on the presence of content changes and the nature of the file operation to minimize redundant I/O.
  let result = if is_deletion {
    let source = read_source_mapped(fs, source_path)?;
    // To ensure the patch applies even on deletion, we apply to a sink.
    apply(&mut sink(), patch, source.as_deref().unwrap_or(&[]))?;

    if !source_path.is_dev_null() {
      fs.remove(source_path)?;
    }
    Ok(())
  } else if has_content {
    if patch.old_file.is_dev_null() {
      ensure_not_exists(fs, &patch.new_file)?;
    }

    let source = read_source_mapped(fs, source_path)?;
    let mut writer = fs.write(&patch.new_file)?;
    apply(&mut writer, patch, source.as_deref().unwrap_or(&[]))?;
    // Explicitly drop source to release memory mapping before attempting to persist (rename) the file.
    // On Windows, an open memory mapping prevents file renaming/moving.
    drop(source);
    writer.commit()?;

    if patch.rename_to.is_some() && patch.new_file != source_path {
      fs.remove(source_path).ignore_not_found()?;
    }
    Ok(())
  } else {
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

  if !patch.new_file.is_dev_null() {
    if let Some(mode) = patch.new_mode {
      fs.set_permissions(&patch.new_file, mode)?;
    }
  }

  Ok(())
}
