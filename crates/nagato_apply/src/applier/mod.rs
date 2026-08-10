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

  let mut first = true;
  while let Some(hunk) = parser.next_hunk(patch)? {
    if first {
      if !hunk.has_header {
        // Hunkless patches need every hunk up front, so fall back to the
        // buffered path once the first one turns out to be headerless.
        patch.hunks.push(hunk);
        while let Some(h) = parser.next_hunk(patch)? {
          patch.hunks.push(h);
        }
        applier.process_hunkless_patches(patch)?;
        return applier.end(patch);
      }
      first = false;
    }
    applier.process_hunk(patch, &hunk)?;
  }

  applier.end(patch)
}

pub fn patch_file(
  fs: &FileSystem,
  patch: Patch<'_>,
  reverse: bool,
) -> Result<(), Error> {
  let patch = if reverse { patch.invert() } else { patch };
  fs::patch_file(fs, &patch)
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
      patch_file(fs, patch?, reverse)?;
    }
    return Ok(());
  }

  while let Some(mut patch) = parser.parse_patch_header()? {
    fs::patch_file_streamed(fs, &mut patch, &mut parser)?;
  }
  Ok(())
}
