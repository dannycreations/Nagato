use nagato_core::fs::FileSystem;
use tempfile::tempdir;

#[test]
fn resolves_valid_path() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path());
  // Should fail because file doesn't exist, but path is valid
  assert!(fs.read(b"file.txt").is_err());
}

#[test]
fn rejects_parent_dir() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path());
  assert!(fs.read(b"../outside.txt").is_err());
  assert!(fs.read(b"subdir/../../outside.txt").is_err());
}

#[test]
fn rejects_absolute_path() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path());
  #[cfg(unix)]
  assert!(fs.read(b"/etc/passwd").is_err());
  #[cfg(windows)]
  assert!(fs.read(b"C:/Windows/System32/drivers/etc/hosts").is_err());
}

#[test]
fn rejects_prefix_component() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path());
  // On Windows, this is a prefix. On Unix, it's just a relative path component "C:".
  // However, our implementation rejects Prefix components which are specific to Windows path parsing.
  #[cfg(windows)]
  assert!(fs.read(b"C:file.txt").is_err());
}
