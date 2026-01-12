use std::{ffi::OsString, io::Write, path::PathBuf};

use nagato_apply::Parser;
use nagato_core::{AtomicWriter, Error};

use super::source::PatchSource;

pub fn process_trim(files: &[OsString], split: bool) -> Result<(), Error> {
  for source_res in PatchSource::iter(files.to_vec()) {
    let source = source_res?;
    let patches: Vec<_> = Parser::new(source.content())
      .collect::<Result<Vec<_>, _>>()
      .map_err(|e| e.with_origin(source.name().to_string()))?;

    // We only support trimming files as we need a base path for output.
    // Stdin support for trim could be added later if we define a default output (e.g. stdout).
    let path = match &source {
      PatchSource::File { name, .. } => PathBuf::from(name.as_ref()),
      PatchSource::Stdin(_) => continue,
    };

    if split {
      for patch in patches {
        let target = String::from_utf8_lossy(patch.filename());
        let out_path = path.with_file_name(format!("{}.trim.patch", target));
        let mut writer = AtomicWriter::new(&out_path)?;
        patch.to_bytes(&mut writer)?;
        writer.commit()?;
      }
    } else {
      let ext = path
        .extension()
        .map(|e| format!("trim.{}", e.to_string_lossy()))
        .unwrap_or_else(|| "trim.patch".to_string());
      let out_path = path.with_extension(ext);

      let mut writer = AtomicWriter::new(&out_path)?;
      for (i, patch) in patches.iter().enumerate() {
        if i > 0 {
          writer.write_all(b"\n")?;
        }
        patch.to_bytes(&mut writer)?;
      }
      writer.commit()?;
    }
  }
  Ok(())
}
