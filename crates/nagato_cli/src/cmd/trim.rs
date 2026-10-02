use std::{
  ffi::OsString,
  fs,
  io::Write,
  path::{Path, PathBuf},
};

use nagato_core::{get_unique_path, AtomicWriter, Error};

use crate::cmd::source::PatchSource;

pub fn process_trim(
  files: Vec<OsString>,
  directory: Option<PathBuf>,
) -> Result<(), Error> {
  if let Some(dir) = directory.as_deref() {
    fs::create_dir_all(dir)?;
  }

  for source_res in PatchSource::iter(files) {
    let source = source_res?;

    let name = match &source {
      PatchSource::File { name, .. } => name,
      PatchSource::Stdin(_) => continue,
    };
    let source_path = Path::new(name.as_ref());

    // `get_unique_path` already returns `dir` joined with the resolved name,
    // so the result must not be joined onto the parent a second time.
    let dir = match directory.as_deref() {
      Some(dir) => dir,
      None => source_path.parent().unwrap_or_else(|| Path::new(".")),
    };

    // A path such as "/" or ".." has no file stem, so fall back to the path
    // itself rather than panicking on a name the user can legitimately pass.
    let stem = source_path
      .file_stem()
      .unwrap_or(source_path.as_os_str())
      .to_string_lossy();
    let base_name = match source_path.extension() {
      Some(ext) => format!("{}.trim.{}", stem, ext.to_string_lossy()),
      None => format!("{}.trim.patch", stem),
    };

    let out_path = get_unique_path(dir, &base_name);

    let mut writer = AtomicWriter::new(&out_path)?;
    for (i, patch_res) in source.patches().enumerate() {
      let patch = patch_res?;
      if i > 0 {
        writer.write_all(b"\n")?;
      }
      patch.write_to(&mut writer)?;
    }
    writer.commit()?;
  }
  Ok(())
}
