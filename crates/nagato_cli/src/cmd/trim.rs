use std::{
  ffi::OsString,
  io::Write,
  path::{Path, PathBuf},
};

use nagato_core::{ensure_dir, get_unique_path, AtomicWriter, Error};

use crate::cmd::{source::PatchSource, utils::parse_patches};

pub fn process_trim(
  files: Vec<OsString>,
  directory: Option<PathBuf>,
) -> Result<(), Error> {
  if let Some(dir) = directory.as_deref() {
    ensure_dir(dir)?;
  }

  for source_res in PatchSource::iter(files) {
    let source = source_res?;
    let patches_iter = parse_patches(&source)?;

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
    let out_path = match directory.as_ref() {
      Some(dir) => get_unique_path(dir, &base_name_str),
      None => {
        let parent = source_path.parent().unwrap_or_else(|| Path::new("."));
        let unique = get_unique_path(parent, &base_name_str);
        parent.join(unique)
      }
    };

    let mut writer = AtomicWriter::new(&out_path)?;
    for (i, patch_res) in patches_iter.enumerate() {
      let patch = patch_res?;
      if i > 0 {
        writer.write_all(b"\n")?;
      }
      patch.to_bytes(&mut writer)?;
    }
    writer.commit()?;
  }
  Ok(())
}
