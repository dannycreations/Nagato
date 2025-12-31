use std::{
  ffi::OsString,
  fs::{self, File},
  io::Read,
  path::PathBuf,
};

use nagato_apply::Parser;
use nagato_core::{Error, ErrorKind};

/// Processes the trim command for the given files.
pub fn process_trim(files: &[OsString]) -> Result<(), Error> {
  for path in files {
    let file_name = path.to_string_lossy().to_string();
    let mut file = File::open(path).map_err(|e| {
      Error::new(ErrorKind::CantOpenPatch(file_name.clone(), e))
    })?;

    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let mut trimmed_content = Vec::new();
    for (i, patch_result) in Parser::new(&content).enumerate() {
      let patch = patch_result?;
      if i > 0 {
        trimmed_content.push(b'\n');
      }
      patch.to_bytes(&mut trimmed_content)?;
    }

    let mut output_path = PathBuf::from(path);
    let mut new_extension = output_path
      .extension()
      .map(|e| e.to_string_lossy().to_string())
      .unwrap_or_default();

    if new_extension.is_empty() {
      new_extension = "trim.patch".to_string();
    } else {
      new_extension = format!("trim.{}", new_extension);
    }
    output_path.set_extension(new_extension);

    fs::write(&output_path, &trimmed_content)?;
  }
  Ok(())
}
