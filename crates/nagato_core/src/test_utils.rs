pub use tempfile;

#[macro_export]
macro_rules! create_test_fs {
  { $($path:expr => $content:expr),* } => {
    {
      let dir = $crate::test_utils::tempfile::Builder::new()
        .prefix("test")
        .tempdir()
        .unwrap();
      $(
        let file_path = dir.path().join($path);
        if let Some(parent) = file_path.parent() {
          ::std::fs::create_dir_all(parent).unwrap();
        }
        let content: &[u8] = $content.as_ref();
        ::std::fs::write(file_path, content).unwrap();
      )*
      dir
    }
  };
}
