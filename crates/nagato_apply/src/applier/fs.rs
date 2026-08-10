use std::io::sink;

use memmap2::Mmap;
use nagato_core::{Error, ErrorKind, FileSystem, IgnoreNotFound, IsDevNull};

use crate::{applier::apply_streamed, apply, Parser, Patch};

fn read_source_mapped(
  fs: &FileSystem,
  path: &[u8],
) -> Result<Option<Mmap>, Error> {
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

fn finish(
  fs: &FileSystem,
  patch: &Patch<'_>,
  result: Result<(), Error>,
) -> Result<(), Error> {
  result.map_err(|e| e.with_file(String::from_utf8_lossy(patch.filename())))?;

  if patch.new_file.is_dev_null() {
    return Ok(());
  }

  match patch.new_mode {
    Some(mode) => fs.set_permissions(&patch.new_file, mode),
    None => Ok(()),
  }
}

fn drop_renamed_source(
  fs: &FileSystem,
  patch: &Patch<'_>,
  source_path: &[u8],
) -> Result<(), Error> {
  if patch.rename_to.is_none() || patch.new_file == source_path {
    return Ok(());
  }
  fs.remove(source_path).ignore_not_found()
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
    (true, _) => apply_deletion(fs, patch, source_path),
    (false, true) => apply_content_change(fs, patch, source_path),
    (false, false) => apply_structural_change(fs, patch, source_path),
  };

  finish(fs, patch, result)
}

pub fn patch_file_streamed<'a>(
  fs: &FileSystem,
  patch: &mut Patch<'a>,
  parser: &mut Parser<'a>,
) -> Result<(), Error> {
  let result = if patch.new_file.is_dev_null() {
    stream_deletion(fs, patch, parser)
  } else if !patch.binary_fragments.is_empty() {
    // Binary payloads are already fully buffered by the header parse.
    return patch_file(fs, patch);
  } else {
    stream_content_change(fs, patch, parser)
  };

  finish(fs, patch, result)
}

fn stream_deletion<'a>(
  fs: &FileSystem,
  patch: &mut Patch<'a>,
  parser: &mut Parser<'a>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  let source = read_source_mapped(fs, source_path)?;
  // The patch is still applied to a sink so that a mismatching hunk is
  // reported instead of silently deleting the file.
  let res = apply_streamed(
    &mut sink(),
    patch,
    source.as_deref().unwrap_or(&[]),
    parser,
  );
  // Release the mapping before unlinking; Windows refuses to remove a file
  // that still has a live mapping.
  drop(source);
  res?;

  let source_path = patch.source_file();
  if source_path.is_dev_null() {
    return Ok(());
  }
  fs.remove(source_path)
}

fn stream_content_change<'a>(
  fs: &FileSystem,
  patch: &mut Patch<'a>,
  parser: &mut Parser<'a>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  let source = read_source_mapped(fs, source_path)?;
  let mut writer = fs.write(&patch.new_file)?;
  let res = apply_streamed(
    &mut writer,
    patch,
    source.as_deref().unwrap_or(&[]),
    parser,
  );
  // Explicitly drop source to release memory mapping before attempting to persist (rename) the file.
  // On Windows, an open memory mapping prevents file renaming/moving.
  drop(source);
  res?;

  writer.commit()?;
  drop_renamed_source(fs, patch, patch.source_file())
}

fn apply_deletion(
  fs: &FileSystem,
  patch: &Patch<'_>,
  source_path: &[u8],
) -> Result<(), Error> {
  let source = read_source_mapped(fs, source_path)?;
  // To ensure the patch applies even on deletion, we apply to a sink.
  let res = apply(&mut sink(), patch, source.as_deref().unwrap_or(&[]));
  // Release the mapping before unlinking; Windows refuses to remove a file
  // that still has a live mapping.
  drop(source);
  res?;

  if source_path.is_dev_null() {
    return Ok(());
  }
  fs.remove(source_path)
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
  let res = apply(&mut writer, patch, source.as_deref().unwrap_or(&[]));
  // Explicitly drop source to release memory mapping before attempting to persist (rename) the file.
  // On Windows, an open memory mapping prevents file renaming/moving.
  drop(source);
  res?;

  writer.commit()?;
  drop_renamed_source(fs, patch, source_path)
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
