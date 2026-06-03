use nagato_core::{Error, ErrorKind, FileSystem, IgnoreNotFound, IsDevNull};

use crate::Patch;

fn read_source_mapped(
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

pub fn patch_file(fs: &FileSystem, patch: &Patch<'_>) -> Result<(), Error> {
  if patch.binary && !patch.hunks.is_empty() {
    return Err(Error::new(ErrorKind::UnsupportedBinaryPatch));
  }

  let is_deletion = patch.new_file.is_dev_null();
  let has_content = patch.has_content_changes();
  let source_path = patch.source_file();

  // Patch application logic is dispatched based on the presence of content changes and the nature of the file operation to minimize redundant I/O.
  let result = match (is_deletion, has_content) {
    (true, _) => self::apply_deletion(fs, patch, source_path),
    (false, true) => self::apply_content_change(fs, patch, source_path),
    (false, false) => self::apply_structural_change(fs, patch, source_path),
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

pub fn patch_file_streamed<'a>(
  fs: &FileSystem,
  patch: &mut Patch<'a>,
  parser: &mut crate::Parser<'a>,
) -> Result<(), Error> {
  let is_deletion = patch.new_file.is_dev_null();
  let source_path = patch.source_file();

  let res = if is_deletion {
    let source = read_source_mapped(fs, source_path)?;
    let mut sink = std::io::sink();
    crate::apply_streamed(
      &mut sink,
      patch,
      source.as_deref().unwrap_or(&[]),
      parser,
    )?;

    if !patch.source_file().is_dev_null() {
      fs.remove(patch.source_file())?;
    }
    Ok(())
  } else if !patch.binary_fragments.is_empty() {
    patch_file(fs, patch)
  } else {
    // Normal content change or structural
    let source = read_source_mapped(fs, source_path)?;
    let mut writer = fs.write(&patch.new_file)?;
    let res = crate::apply_streamed(
      &mut writer,
      patch,
      source.as_deref().unwrap_or(&[]),
      parser,
    );
    drop(source);
    if let Ok(()) = res {
      writer.commit()?;
      let source = patch.source_file();
      if patch.rename_to.is_some() && patch.new_file != source {
        fs.remove(source).ignore_not_found()?;
      }
    }
    res
  };

  res.map_err(|e: Error| {
    e.with_file(String::from_utf8_lossy(patch.filename()))
  })?;

  if !patch.new_file.is_dev_null() {
    if let Some(mode) = patch.new_mode {
      fs.set_permissions(&patch.new_file, mode)?;
    }
  }

  Ok(())
}

fn apply_deletion(
  fs: &FileSystem,
  patch: &Patch<'_>,
  source_path: &[u8],
) -> Result<(), Error> {
  let source = read_source_mapped(fs, source_path)?;
  // To ensure the patch applies even on deletion, we apply to a sink.
  crate::apply(
    &mut std::io::sink(),
    patch,
    source.as_deref().unwrap_or(&[]),
  )?;

  if !source_path.is_dev_null() {
    fs.remove(source_path)?;
  }
  Ok(())
}

fn apply_content_change(
  fs: &FileSystem,
  patch: &Patch<'_>,
  source_path: &[u8],
) -> Result<(), Error> {
  if patch.old_file.is_dev_null() {
    ensure_not_exists(fs, &patch.new_file)?;
  }

  let source = read_source_mapped(fs, source_path)?;
  let mut writer = fs.write(&patch.new_file)?;
  crate::apply(&mut writer, patch, source.as_deref().unwrap_or(&[]))?;
  // Explicitly drop source to release memory mapping before attempting to persist (rename) the file.
  // On Windows, an open memory mapping prevents file renaming/moving.
  drop(source);
  writer.commit()?;

  if patch.rename_to.is_some() && patch.new_file != source_path {
    fs.remove(source_path).ignore_not_found()?;
  }
  Ok(())
}

fn apply_structural_change(
  fs: &FileSystem,
  patch: &Patch<'_>,
  source_path: &[u8],
) -> Result<(), Error> {
  // Structural changes like renames, copies, or file creations are handled by mapping the intended operation to the corresponding filesystem primitive.
  if patch.rename_to.is_some() {
    return fs.rename(source_path, &patch.new_file);
  }

  if patch.copy_to.is_some() {
    return fs.copy(source_path, &patch.new_file);
  }

  if patch.old_file.is_dev_null() && !patch.new_file.is_dev_null() {
    ensure_not_exists(fs, &patch.new_file)?;
    fs.write(&patch.new_file)?.commit()?;
  }

  Ok(())
}
