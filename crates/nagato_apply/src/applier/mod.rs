use std::io::Write;

use nagato_core::{Error, FileSystem};

pub mod engine;
pub mod fs;
pub mod matcher;

pub use engine::Applier;

use crate::{Parser, Patch};

pub fn apply(
  output: &mut (impl Write + ?Sized),
  patch: &Patch<'_>,
  source: &[u8],
) -> Result<(), Error> {
  if !patch.has_content_changes() && patch.copy_to.is_none() {
    output.write_all(source)?;
    return Ok(());
  }
  Applier::new(output, source).process(patch)
}

pub(crate) fn apply_streamed<'a>(
  output: &mut (impl Write + ?Sized),
  patch: &mut Patch<'a>,
  source: &[u8],
  parser: &mut Parser<'a>,
) -> Result<(), Error> {
  let mut applier = Applier::new(output, source);
  applier.begin(patch)?;

  if !patch.binary_fragments.is_empty() {
    return applier.process_binary(patch);
  }

  // The first hunk decides the mode: a headerless one needs every hunk up
  // front, so the patch falls back to the buffered path. Later hunks are
  // streamed one by one no matter what shape they have.
  let Some(first_hunk) = parser.next_hunk(patch)? else {
    return applier.end(patch);
  };

  if !first_hunk.has_header {
    patch.hunks.push(first_hunk);
    while let Some(hunk) = parser.next_hunk(patch)? {
      patch.hunks.push(hunk);
    }
    applier.process_hunkless_patches(patch)?;
    return applier.end(patch);
  }

  applier.process_hunk(patch, &first_hunk)?;

  while let Some(hunk) = parser.next_hunk(patch)? {
    applier.process_hunk(patch, &hunk)?;
  }

  applier.end(patch)
}

pub fn patch_file(
  fs: &FileSystem,
  patch: Patch<'_>,
  reverse: bool,
) -> Result<(), Error> {
  let mut patch = if reverse { patch.invert() } else { patch };
  fs::patch_file(fs, &mut patch)
}

pub fn apply_to_fs(
  fs: &FileSystem,
  input: &[u8],
  reverse: bool,
) -> Result<(), Error> {
  let mut parser = Parser::new(input);

  // Reversed patches must be fully buffered before they can be inverted, so
  // only the forward direction can be streamed hunk by hunk.
  if reverse {
    for patch in parser {
      fs::patch_file(fs, &mut patch?.invert())?;
    }
    return Ok(());
  }

  while let Some(mut patch) = parser.parse_patch_header()? {
    fs::patch_file_streamed(fs, &mut patch, &mut parser)?;
  }
  Ok(())
}
