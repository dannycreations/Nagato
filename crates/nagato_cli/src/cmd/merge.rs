use std::{collections::HashMap, ffi::OsString, io::Write, mem, path::PathBuf};

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
    let parser = Parser::new(source.content());

    for patch_res in parser {
      let patch =
        patch_res.map_err(|e| e.with_origin(source.name().to_string()))?;
      let filename = patch.filename();

      if let Some(existing_patch) = merged_patches.get_mut(filename) {
        let mut hunks = mem::take(&mut existing_patch.hunks).into_vec();
        hunks.extend(Vec::from(patch.hunks));
        existing_patch.hunks = hunks.into_boxed_slice();

        let mut frags =
          mem::take(&mut existing_patch.binary_fragments).into_vec();
        frags.extend(Vec::from(patch.binary_fragments));
        existing_patch.binary_fragments = frags.into_boxed_slice();
      } else {
        let filename_vec = filename.to_vec();
        filenames_order.push(filename_vec.clone());
        merged_patches.insert(filename_vec, patch);
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
