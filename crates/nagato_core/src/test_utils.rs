use std::{
  fs::{create_dir_all, write},
  path::Path,
};

pub use tempfile;
use tempfile::{Builder, TempDir};

pub fn create_temp_dir() -> TempDir {
  Builder::new().prefix("test").tempdir().unwrap()
}

pub fn create_dir_all_helper(path: &Path) {
  create_dir_all(path).unwrap();
}

pub fn write_file_helper(path: &Path, content: &[u8]) {
  write(path, content).unwrap();
}

#[macro_export]
macro_rules! create_test_fs {
  { $($path:expr => $content:expr),* } => {
    {
      let dir = $crate::test_utils::create_temp_dir();
      $(
        let file_path = dir.path().join($path);
        if let Some(parent) = file_path.parent() {
          $crate::test_utils::create_dir_all_helper(parent);
        }
        let content: &[u8] = $content.as_ref();
        $crate::test_utils::write_file_helper(&file_path, content);
      )*
      dir
    }
  };
}
