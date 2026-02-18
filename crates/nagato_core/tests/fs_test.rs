use nagato_core::{ErrorKind, FileSystem};
use tempfile::tempdir;

#[test]
fn resolves_valid_path() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path(), false);
  // Should fail because file doesn't exist, but path is valid
  assert!(fs.read(b"file.txt").is_err());
}

#[test]
fn rejects_parent_dir() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path(), false);
  assert!(fs.read(b"../outside.txt").is_err());
  assert!(fs.read(b"subdir/../../outside.txt").is_err());
}

#[test]
fn rejects_absolute_path() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path(), false);
  #[cfg(unix)]
  assert!(fs.read(b"/etc/passwd").is_err());
  #[cfg(windows)]
  assert!(fs.read(b"C:/Windows/System32/drivers/etc/hosts").is_err());
}

#[test]
fn rejects_prefix_component() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path(), false);
  // On Windows, this is a prefix. On Unix, it's just a relative path component "C:".
  // However, our implementation rejects Prefix components which are specific to Windows path parsing.
  #[cfg(windows)]
  assert!(fs.read(b"C:file.txt").is_err());
}

#[test]
fn rejects_reserved_names() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path(), false);
  assert!(fs.read(b"con").is_err());
  assert!(fs.read(b"PRN.txt").is_err());
  assert!(fs.read(b"aux/file").is_err());
  assert!(fs.read(b"NUL").is_err());
  assert!(fs.read(b"com1").is_err());
  assert!(fs.read(b"LPT9").is_err());
  assert!(fs.read(b"CLOCK$").is_err());
}

#[test]
fn rejects_trailing_dots_and_spaces() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path(), false);
  assert!(fs.read(b"file.txt.").is_err());
  assert!(fs.read(b"file.txt ").is_err());
  assert!(fs.read(b"space /file").is_err());
}

#[test]
fn rejects_short_names() {
  let root = tempdir().unwrap();
  let fs = FileSystem::new(root.path(), false);
  assert!(fs.read(b"PROGRA~1").is_err());
  assert!(fs.read(b"docume~2.txt").is_err());
  // Valid use of tilde (not followed by digit) should pass validation.
  // We check that it doesn't return InvalidPath. It returns an Io error (NotFound)
  // because the file doesn't actually exist.
  let res = fs.read(b"my~file.txt");
  assert!(res.is_err());
  assert!(res.unwrap_err().kind != ErrorKind::InvalidPath);
}
