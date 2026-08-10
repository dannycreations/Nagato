use std::{
  ffi::OsString,
  path::{Path, PathBuf},
};

use nagato_core::{ensure_dir, get_unique_path, AtomicWriter, Error};

use crate::cmd::source::PatchSource;

pub fn process_split(
  files: Vec<OsString>,
  directory: Option<PathBuf>,
) -> Result<(), Error> {
  if let Some(dir) = directory.as_deref() {
    ensure_dir(dir)?;
  }

  for source_res in PatchSource::iter(files) {
    let source = source_res?;

    for patch_res in source.patches() {
      let patch = patch_res?;
      let target = String::from_utf8_lossy(patch.filename());
      let file_name = Path::new(target.as_ref())
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| target.clone());
      let base_name = format!("{}.trim.patch", file_name);

      let dir = directory.as_deref().unwrap_or_else(|| Path::new("."));
      let out_path = get_unique_path(dir, &base_name);

      let mut writer = AtomicWriter::new(&out_path)?;
      patch.write_to(&mut writer)?;
      writer.commit()?;
    }
  }
  Ok(())
}
