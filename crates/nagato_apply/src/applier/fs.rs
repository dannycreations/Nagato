use std::io::sink;

use memmap2::Mmap;
use nagato_core::{Error, ErrorKind, FileSystem, IsDevNull};

use crate::{applier::apply_streamed, apply, Parser, Patch};

// Each caller releases the mapping before unlinking or renaming the file;
// Windows refuses both operations while a memory mapping of it is alive.
fn read_source_mapped(
  fs: &FileSystem,
  path: &[u8],
) -> Result<Option<Mmap>, Error> {
  if path.is_dev_null() {
    return Ok(None);
  }
  match fs.read(path) {
    Ok(source) => Ok(Some(source)),
    Err(e) if e.is_not_found() => Ok(None),
    Err(e) => Err(e),
  }
}

fn ensure_not_exists(fs: &FileSystem, path: &[u8]) -> Result<(), Error> {
  if fs.exists(path) {
    Err(Error::new(ErrorKind::AlreadyExists))
  } else {
    Ok(())
  }
}

fn remove_source(fs: &FileSystem, source_path: &[u8]) -> Result<(), Error> {
  if source_path.is_dev_null() {
    return Ok(());
  }
  fs.remove(source_path)
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
  match fs.remove(source_path) {
    Err(e) if e.is_not_found() => Ok(()),
    res => res,
  }
}

pub fn patch_file(fs: &FileSystem, patch: &Patch<'_>) -> Result<(), Error> {
  if patch.binary && !patch.hunks.is_empty() {
    return Err(Error::new(ErrorKind::UnsupportedBinaryPatch));
  }

  let is_deletion = patch.new_file.is_dev_null();
  let has_content = patch.has_content_changes();
  let source_path = patch.source_file();

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
  // Owned so that the closure below can take `patch` mutably.
  let source_path = patch.source_file().to_vec();

  // Applied to a sink so that a mismatching hunk is reported instead of
  // silently deleting the file.
  let applied = read_source_mapped(fs, &source_path).and_then(|source| {
    apply_streamed(&mut sink(), patch, source.as_deref().unwrap_or(&[]), parser)
  });
  applied?;

  remove_source(fs, &source_path)
}

fn stream_content_change<'a>(
  fs: &FileSystem,
  patch: &mut Patch<'a>,
  parser: &mut Parser<'a>,
) -> Result<(), Error> {
  if patch.old_file.is_dev_null() {
    ensure_not_exists(fs, &patch.new_file)?;
  }

  let source_path = patch.source_file().to_vec();

  let writer = read_source_mapped(fs, &source_path).and_then(|source| {
    let mut writer = fs.write(&patch.new_file)?;
    apply_streamed(&mut writer, patch, source.as_deref().unwrap_or(&[]), parser)
      .map(|_| writer)
  })?;
  writer.commit()?;

  drop_renamed_source(fs, patch, &source_path)
}

fn apply_deletion(
  fs: &FileSystem,
  patch: &Patch<'_>,
  source_path: &[u8],
) -> Result<(), Error> {
  // Applied to a sink so that a mismatching hunk is reported instead of
  // silently deleting the file.
  let applied = read_source_mapped(fs, source_path).and_then(|source| {
    apply(&mut sink(), patch, source.as_deref().unwrap_or(&[]))
  });
  applied?;

  remove_source(fs, source_path)
}

fn apply_content_change(
  fs: &FileSystem,
  patch: &Patch<'_>,
  source_path: &[u8],
) -> Result<(), Error> {
  if patch.old_file.is_dev_null() {
    ensure_not_exists(fs, &patch.new_file)?;
  }

  let writer = read_source_mapped(fs, source_path).and_then(|source| {
    let mut writer = fs.write(&patch.new_file)?;
    apply(&mut writer, patch, source.as_deref().unwrap_or(&[])).map(|_| writer)
  })?;
  writer.commit()?;

  drop_renamed_source(fs, patch, source_path)
}

fn apply_structural_change(
  fs: &FileSystem,
  patch: &Patch<'_>,
  source_path: &[u8],
) -> Result<(), Error> {
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
