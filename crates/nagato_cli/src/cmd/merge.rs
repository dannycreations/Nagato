use std::{collections::HashMap, ffi::OsString, io::Write, path::PathBuf};

use nagato_apply::Patch;
use nagato_core::{AtomicWriter, Error};

use crate::cmd::{source::PatchSource, utils::parse_patches};

pub fn process_merge(
  files: &[OsString],
  output: Option<PathBuf>,
) -> Result<(), Error> {
  let mut merged_patches: HashMap<Vec<u8>, Patch> = HashMap::new();
  let mut filenames_order: Vec<Vec<u8>> = Vec::new();

  let sources: Vec<PatchSource> =
    PatchSource::iter(files.to_vec()).collect::<Result<_, _>>()?;

  for source in &sources {
    for patch_res in parse_patches(source)? {
      let patch = patch_res?;
      let filename = patch.filename();

      match merged_patches.get_mut(filename) {
        Some(existing_patch) => existing_patch.append(patch),
        None => {
          let filename_vec = filename.to_vec();
          filenames_order.push(filename_vec.clone());
          merged_patches.insert(filename_vec, patch);
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
      patch.to_bytes(&mut writer)?;
    }
  }

  writer.commit()?;
  Ok(())
}
