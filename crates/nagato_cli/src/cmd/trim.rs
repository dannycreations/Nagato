use std::{
  ffi::OsString,
  io::Write,
  path::{Path, PathBuf},
};

use nagato_core::{ensure_dir, get_unique_path, AtomicWriter, Error};

use crate::cmd::source::PatchSource;

pub fn process_trim(
  files: Vec<OsString>,
  directory: Option<PathBuf>,
) -> Result<(), Error> {
  if let Some(dir) = directory.as_deref() {
    ensure_dir(dir)?;
  }

  for source_res in PatchSource::iter(files) {
    let source = source_res?;

    let source_path = match &source {
      PatchSource::File { name, .. } => PathBuf::from(name.as_ref()),
      PatchSource::Stdin(_) => continue,
    };

    let ext = source_path
      .extension()
      .map(|e| format!("trim.{}", e.to_string_lossy()))
      .unwrap_or_else(|| "trim.patch".to_string());
    let base_name = source_path
      .with_extension(ext)
      .file_name()
      .unwrap()
      .to_os_string();

    let base_name_str = base_name.to_string_lossy();
    // `get_unique_path` already returns `dir` joined with the resolved name,
    // so the result must not be joined onto the parent a second time.
    let dir = match directory.as_deref() {
      Some(dir) => dir,
      None => source_path.parent().unwrap_or_else(|| Path::new(".")),
    };
    let out_path = get_unique_path(dir, &base_name_str);

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
