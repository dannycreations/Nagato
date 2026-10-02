use std::{collections::HashMap, ffi::OsString, io::Write, path::PathBuf};

use nagato_apply::Patch;
use nagato_core::{AtomicWriter, Error};

use crate::cmd::source::PatchSource;

pub fn process_merge(
  files: Vec<OsString>,
  output: Option<PathBuf>,
) -> Result<(), Error> {
  let sources: Vec<PatchSource> =
    PatchSource::iter(files).collect::<Result<_, _>>()?;

  // Patches keep their first-seen order in `merged`, while `by_filename` maps
  // a file name to the slot holding every patch for it.
  let mut merged: Vec<Patch> = Vec::new();
  let mut by_filename: HashMap<Vec<u8>, usize> = HashMap::new();

  for source in &sources {
    for patch_res in source.patches() {
      let patch = patch_res?;
      let filename = patch.filename();

      match by_filename.get(filename) {
        Some(&slot) => merged[slot].append(patch),
        None => {
          by_filename.insert(filename.to_vec(), merged.len());
          merged.push(patch);
        }
      }
    }
  }

  let out_path = output.unwrap_or_else(|| PathBuf::from("merge.patch"));
  let mut writer = AtomicWriter::new(&out_path)?;

  for (i, patch) in merged.iter().enumerate() {
    if i > 0 {
      writer.write_all(b"\n")?;
    }
    patch.write_to(&mut writer)?;
  }

  writer.commit()?;
  Ok(())
}
