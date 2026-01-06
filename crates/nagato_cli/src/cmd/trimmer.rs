use std::{
  ffi::OsString,
  fs,
  fs::File,
  io::Result as IoResult,
  path::{Path, PathBuf},
};

use memmap2::Mmap;
use nagato_apply::Parser;
use nagato_core::{Error, ErrorKind};

/// Processes the trim command for the given files.
pub fn process_trim(files: &[OsString], split: bool) -> Result<(), Error> {
  for path in files {
    let file_name = path.to_string_lossy().to_string();
    let file = File::open(path).map_err(|e| {
      Error::new(ErrorKind::CantOpenPatch(file_name.clone(), e))
    })?;

    // SAFETY: Use Mmap for consistent, high-performance reading of patch files.
    let content = unsafe { Mmap::map(&file)? };

    let patches: Vec<_> = Parser::new(&content)
      .collect::<Result<Vec<_>, _>>()
      .map_err(|e| e.with_origin(file_name.clone()))?;

    if split {
      for patch in patches {
        let target = String::from_utf8_lossy(patch.filename());
        let mut out_path = PathBuf::from(path);
        out_path.set_file_name(format!("{}.trim.patch", target));

        ensure_parent_dirs(&out_path)?;

        let mut buf = Vec::new();
        patch.to_bytes(&mut buf)?;
        fs::write(out_path, buf)?;
      }
    } else {
      let mut buf = Vec::new();
      for (i, patch) in patches.iter().enumerate() {
        if i > 0 {
          buf.push(b'\n');
        }
        patch.to_bytes(&mut buf)?;
      }

      let mut out_path = PathBuf::from(path);
      let ext = out_path
        .extension()
        .map(|e| format!("trim.{}", e.to_string_lossy()))
        .unwrap_or_else(|| "trim.patch".to_string());
      out_path.set_extension(ext);

      ensure_parent_dirs(&out_path)?;
      fs::write(out_path, buf)?;
    }
  }
  Ok(())
}

fn ensure_parent_dirs(path: &Path) -> IoResult<()> {
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent)?;
  }
  Ok(())
}
