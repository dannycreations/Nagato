use std::{
  ffi::OsString,
  path::{Path, PathBuf},
};

use nagato_core::{ensure_dir, get_unique_path, AtomicWriter, Error};

use super::{source::PatchSource, utils::parse_patches};

pub fn process_split(
  files: &[OsString],
  directory: Option<PathBuf>,
) -> Result<(), Error> {
  if let Some(dir) = directory.as_deref() {
    ensure_dir(dir)?;
  }

  for source_res in PatchSource::iter(files.to_vec()) {
    let source = source_res?;
    let patches = parse_patches(&source)?;

    for patch in patches {
      let target = String::from_utf8_lossy(patch.filename());
      let file_name = Path::new(target.as_ref())
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| target.clone());
      let base_name = format!("{}.trim.patch", file_name);

      let out_path = if let Some(ref dir) = directory {
        get_unique_path(dir, &base_name)
      } else {
        get_unique_path(Path::new("."), &base_name)
      };

      let mut writer = AtomicWriter::new(&out_path)?;
      patch.to_bytes(&mut writer)?;
      writer.commit()?;
    }
  }
  Ok(())
}
