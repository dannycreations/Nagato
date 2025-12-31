use std::{
  ffi::OsString,
  fs::{self, File},
  io::Read,
  path::PathBuf,
};

use nagato_apply::{LineKind, Parser, Patch};
use nagato_core::{Error, ErrorKind};

/// Processes the trim command for the given files.
pub fn process_trim(files: &[OsString]) -> Result<(), Error> {
  for path in files {
    let file_name = path.to_string_lossy().to_string();
    let mut file = File::open(path).map_err(|e| {
      Error::new(ErrorKind::CantOpenPatch(file_name.clone(), e.into()))
    })?;

    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let mut trimmed_content = Vec::new();
    for (i, patch_result) in Parser::new(&content).enumerate() {
      let patch = patch_result?;
      if i > 0 {
        trimmed_content.push(b'\n');
      }
      trim_patch(&patch, &mut trimmed_content)?;
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

/// Trims a single patch and writes it to the output buffer.
fn trim_patch(patch: &Patch, out: &mut Vec<u8>) -> Result<(), Error> {
  // Replace header with `file`
  // We use the new_file as the primary name if available, otherwise old_file.
  let target_file = if !patch.new_file.is_empty() {
    patch.new_file
  } else {
    patch.old_file
  };

  out.extend_from_slice(b"file ");
  out.extend_from_slice(target_file);
  out.push(b'\n');

  for (i, hunk) in patch.hunks.iter().enumerate() {
    // If it's the first hunk and no label, add extra newline after file header
    // Otherwise, add newline between hunks
    if i == 0 {
      if hunk.label.is_none() {
        out.push(b'\n');
      }
    } else {
      out.push(b'\n');
    }

    // Replace hunk header with `label` if label exists
    if let Some(label) = hunk.label {
      out.extend_from_slice(b"label ");
      out.extend_from_slice(label);
      out.push(b'\n');
      out.push(b'\n');
    }

    for line in &hunk.lines {
      let prefix = match line.kind {
        LineKind::Addition => b'+',
        LineKind::Deletion => b'-',
        LineKind::Context => b' ',
      };
      out.push(prefix);
      out.extend_from_slice(line.text);
      out.push(b'\n');
    }
  }

  Ok(())
}
