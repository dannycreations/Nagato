use std::{ffi::OsString, io::Write, path::PathBuf};

use nagato_core::{ensure_dir, get_unique_path, AtomicWriter, Error};

use super::{source::PatchSource, utils::parse_patches};

pub fn process_trim(
  files: &[OsString],
  directory: Option<PathBuf>,
) -> Result<(), Error> {
  if let Some(dir) = directory.as_deref() {
    ensure_dir(dir)?;
  }

  for source_res in PatchSource::iter(files.to_vec()) {
    let source = source_res?;
    let patches = parse_patches(&source)?;

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

    let out_path = if let Some(ref dir) = directory {
      get_unique_path(dir, &base_name.to_string_lossy())
    } else {
      source_path.with_file_name(get_unique_path(
        source_path.parent().unwrap_or(std::path::Path::new(".")),
        &base_name.to_string_lossy(),
      ))
    };

    let mut writer = AtomicWriter::new(&out_path)?;
    for (i, patch) in patches.iter().enumerate() {
      if i > 0 {
        writer.write_all(b"\n")?;
      }
      patch.to_bytes(&mut writer)?;
    }
    writer.commit()?;
  }
  Ok(())
}
