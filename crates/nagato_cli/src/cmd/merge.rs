use std::{
  collections::{hash_map::Entry, HashMap},
  ffi::OsString,
  io::Write,
  path::PathBuf,
};

use nagato_apply::Patch;
use nagato_core::{AtomicWriter, Error};

use crate::cmd::source::PatchSource;

pub fn process_merge(
  files: Vec<OsString>,
  output: Option<PathBuf>,
) -> Result<(), Error> {
  let sources: Vec<PatchSource> =
    PatchSource::iter(files).collect::<Result<_, _>>()?;

  let mut merged_patches: HashMap<Vec<u8>, Patch> = HashMap::new();
  let mut filenames_order: Vec<Vec<u8>> = Vec::new();

  for source in &sources {
    for patch_res in source.patches() {
      let patch = patch_res?;
      let filename = patch.filename();

      match merged_patches.entry(filename.to_vec()) {
        Entry::Occupied(mut entry) => {
          entry.get_mut().append(patch);
        }
        Entry::Vacant(entry) => {
          filenames_order.push(entry.key().clone());
          entry.insert(patch);
        }
      }
    }
  }

  let out_path = output.unwrap_or_else(|| PathBuf::from("merge.patch"));
  let mut writer = AtomicWriter::new(&out_path)?;

  for (i, filename) in filenames_order.iter().enumerate() {
    if i > 0 {
      writer.write_all(b"\n")?;
    }
    if let Some(patch) = merged_patches.get(filename) {
      patch.write_to(&mut writer)?;
    }
  }

  writer.commit()?;
  Ok(())
}
