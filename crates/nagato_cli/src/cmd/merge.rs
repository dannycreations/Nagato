use std::{collections::HashMap, ffi::OsString, io::Write, path::PathBuf};

use nagato_apply::{Parser, Patch};
use nagato_core::{AtomicWriter, Error};

use super::source::PatchSource;

pub fn process_merge(
  files: &[OsString],
  output: Option<PathBuf>,
) -> Result<(), Error> {
  let mut merged_patches: HashMap<Vec<u8>, Patch> = HashMap::new();
  let mut filenames_order: Vec<Vec<u8>> = Vec::new();

  let sources: Vec<PatchSource> =
    PatchSource::iter(files.to_vec()).collect::<Result<Vec<_>, _>>()?;

  for source in &sources {
    let patches: Vec<Patch> = Parser::new(source.content())
      .collect::<Result<Vec<_>, _>>()
      .map_err(|e| e.with_origin(source.name().to_string()))?;

    for patch in patches {
      let filename = patch.filename().to_vec();
      if let Some(existing_patch) = merged_patches.get_mut(&filename) {
        existing_patch.hunks.extend(patch.hunks);
        existing_patch
          .binary_fragments
          .extend(patch.binary_fragments);
      } else {
        filenames_order.push(filename.clone());
        merged_patches.insert(filename, patch);
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
