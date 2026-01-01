use std::{ffi::OsString, fs, fs::File, path::PathBuf};

use memmap2::Mmap;
use nagato_apply::Parser;
use nagato_core::{Error, ErrorKind};

/// Processes the trim command for the given files.
pub fn process_trim(files: &[OsString]) -> Result<(), Error> {
  for path in files {
    let file_name = path.to_string_lossy().to_string();
    let file = File::open(path).map_err(|e| {
      Error::new(ErrorKind::CantOpenPatch(file_name.clone(), e))
    })?;

    // SAFETY: Use Mmap for consistent, high-performance reading of patch files.
    let content = unsafe { Mmap::map(&file)? };

    let mut trimmed_content = Vec::new();
    for (i, patch_result) in Parser::new(&content).enumerate() {
      let patch = patch_result?;
      if i > 0 {
        trimmed_content.push(b'\n');
      }
      patch.to_bytes(&mut trimmed_content)?;
    }

    let mut output_path = PathBuf::from(path);
    let new_extension = match output_path.extension() {
      Some(ext) => format!("trim.{}", ext.to_string_lossy()),
      None => "trim.patch".to_string(),
    };
    output_path.set_extension(new_extension);

    fs::write(&output_path, &trimmed_content)?;
  }
  Ok(())
}
