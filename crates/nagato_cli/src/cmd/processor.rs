use nagato_apply::{patch_file, Parser};
use nagato_core::{Error, FileSystem};

pub fn process_patch(
  fs: &FileSystem,
  patch_content: &[u8],
  reverse: bool,
) -> Result<(), Error> {
  for patch in Parser::new(patch_content) {
    patch_file(fs, patch?, reverse)?;
  }
  Ok(())
}
