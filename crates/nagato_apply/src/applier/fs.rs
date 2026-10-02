use std::io::sink;

use memmap2::Mmap;
use nagato_core::{is_dev_null, AtomicWriter, Error, ErrorKind, FileSystem};

use crate::{applier::apply_streamed, apply, Parser, Patch};

// Each caller releases the mapping before unlinking or renaming the file;
// Windows refuses both operations while a memory mapping of it is alive.
fn read_source_mapped(
  fs: &FileSystem,
  path: &[u8],
) -> Result<Option<Mmap>, Error> {
  if is_dev_null(path) {
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
  if is_dev_null(source_path) {
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

  if is_dev_null(&patch.new_file) {
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

fn rewrite_file<'a>(
  fs: &FileSystem,
  patch: &mut Patch<'a>,
  apply: impl FnOnce(&mut AtomicWriter, &mut Patch<'a>, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
  if is_dev_null(&patch.old_file) {
    ensure_not_exists(fs, &patch.new_file)?;
  }

  // The closure owns the source mapping so that it is released before the
  // destination is replaced, which Windows refuses to do while a mapping of
  // the same file is still alive.
  let writer =
    read_source_mapped(fs, patch.source_file()).and_then(|source| {
      let mut writer = fs.write(&patch.new_file)?;
      apply(&mut writer, patch, source.as_deref().unwrap_or(&[]))
        .map(|_| writer)
    })?;
  writer.commit()?;

  drop_renamed_source(fs, patch, patch.source_file())
}

pub fn patch_file(fs: &FileSystem, patch: &mut Patch<'_>) -> Result<(), Error> {
  if patch.binary && !patch.hunks.is_empty() {
    return Err(Error::new(ErrorKind::UnsupportedBinaryPatch));
  }

  let is_deletion = is_dev_null(&patch.new_file);
  let has_content = patch.has_content_changes();

  let result = match (is_deletion, has_content) {
    (true, _) => apply_deletion(fs, patch),
    (false, true) => apply_content_change(fs, patch),
    (false, false) => apply_structural_change(fs, patch),
  };

  finish(fs, patch, result)
}

pub fn patch_file_streamed<'a>(
  fs: &FileSystem,
  patch: &mut Patch<'a>,
  parser: &mut Parser<'a>,
) -> Result<(), Error> {
  let is_deletion = is_dev_null(&patch.new_file);

  // Binary payloads are already fully buffered by the header parse, so they
  // take the whole-file path rather than the streaming one.
  if !is_deletion && !patch.binary_fragments.is_empty() {
    return patch_file(fs, patch);
  }

  let result = if is_deletion {
    stream_deletion(fs, patch, parser)
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
  apply_then_remove(fs, &source_path, |source| {
    apply_streamed(&mut sink(), patch, source, parser)
  })
}

fn stream_content_change<'a>(
  fs: &FileSystem,
  patch: &mut Patch<'a>,
  parser: &mut Parser<'a>,
) -> Result<(), Error> {
  rewrite_file(fs, patch, |writer, patch, source| {
    apply_streamed(writer, patch, source, parser)
  })
}

fn apply_then_remove(
  fs: &FileSystem,
  source_path: &[u8],
  validate: impl FnOnce(&[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
  // Applied to a sink so that a mismatching hunk is reported instead of
  // silently deleting the file.
  let applied = read_source_mapped(fs, source_path)
    .and_then(|source| validate(source.as_deref().unwrap_or(&[])));
  applied?;

  remove_source(fs, source_path)
}

fn apply_deletion(fs: &FileSystem, patch: &Patch<'_>) -> Result<(), Error> {
  apply_then_remove(fs, patch.source_file(), |source| {
    apply(&mut sink(), patch, source)
  })
}

fn apply_content_change(
  fs: &FileSystem,
  patch: &mut Patch<'_>,
) -> Result<(), Error> {
  rewrite_file(fs, patch, |writer, patch, source| {
    apply(writer, patch, source)
  })
}

fn apply_structural_change(
  fs: &FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  if patch.rename_to.is_some() {
    return fs.rename(patch.source_file(), &patch.new_file);
  }

  if patch.copy_to.is_some() {
    return fs.copy(patch.source_file(), &patch.new_file);
  }

  if is_dev_null(&patch.old_file) && !is_dev_null(&patch.new_file) {
    ensure_not_exists(fs, &patch.new_file)?;
    fs.write(&patch.new_file)?.commit()?;
  }

  Ok(())
}
