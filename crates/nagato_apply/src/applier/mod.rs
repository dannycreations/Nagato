pub mod engine;
pub mod utils;
pub mod worker;

use std::io::Write;

pub use engine::Applier;
use nagato_core::{error::Error, fs::FileSystem};

use crate::models::patch::Patch;

/// Apply a patch to the source bytes and write to the output.
/// This is the core application logic that works on byte slices.
pub fn apply<'a>(
  output: &mut (impl Write + ?Sized),
  patch: &Patch<'a>,
  source: &[u8],
) -> Result<(), Error> {
  if patch.hunks.is_empty()
    && patch.copy_to.is_none()
    && patch.binary_fragments.is_empty()
  {
    output.write_all(source)?;
    return Ok(());
  }
  Applier::new(output, source).process(patch)
}

/// Apply a patch to the file system.
/// This handles file I/O, renames, and deletions using the provided FileSystem.
pub fn patch_file(
  fs: &FileSystem,
  patch: Patch<'_>,
  reverse: bool,
) -> Result<(), Error> {
  if reverse {
    worker::patch_file_worker(fs, &patch.invert())
  } else {
    worker::patch_file_worker(fs, &patch)
  }
}
