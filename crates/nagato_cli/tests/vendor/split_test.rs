use std::fs;

use tempfile::tempdir;

#[test]
fn cli_split_basic() {
  let dir = tempdir().unwrap();
  let patch_file = dir.path().join("multi.patch");
  fs::write(&patch_file, "--- a/f1\n+++ b/f1\n@@ -1 +1 @@\n-1\n+A\n--- a/f2\n+++ b/f2\n@@ -1 +1 @@\n-2\n+B\n").unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.current_dir(dir.path()).arg("split").arg("multi.patch");

  cmd.assert().success();
  assert!(dir.path().join("f1.trim.patch").exists());
  assert!(dir.path().join("f2.trim.patch").exists());
}

#[test]
fn cli_split_custom_directory() {
  let dir = tempdir().unwrap();
  let patch_file = dir.path().join("test.patch");
  fs::write(&patch_file, "--- a/f\n+++ b/f\n").unwrap();

  let out_dir = dir.path().join("out");

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd
    .current_dir(dir.path())
    .arg("split")
    .arg("-d")
    .arg("out")
    .arg("test.patch");

  cmd.assert().success();
  assert!(out_dir.join("f.trim.patch").exists());
}

#[test]
fn cli_split_conflict_resolution() {
  let dir = tempdir().unwrap();
  let existing = dir.path().join("f.trim.patch");
  fs::write(&existing, "existing").unwrap();

  let patch_file = dir.path().join("test.patch");
  fs::write(&patch_file, "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-1\n+A\n").unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.current_dir(dir.path()).arg("split").arg("test.patch");

  cmd.assert().success();
  assert_eq!(fs::read_to_string(existing).unwrap(), "existing");
  assert!(dir.path().join("f-1.trim.patch").exists());
}

#[test]
fn cli_split_stdin() {
  let dir = tempdir().unwrap();
  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd
    .current_dir(dir.path())
    .arg("split")
    .write_stdin("--- a/f\n+++ b/f\n");

  cmd.assert().success();
  assert!(dir.path().join("f.trim.patch").exists());
}

#[test]
fn cli_split_mixed_and_edge() {
  let dir = tempdir().unwrap();
  let patch_file = dir.path().join("mixed.patch");
  fs::write(&patch_file, "diff --git a/text b/text\n--- a/text\n+++ b/text\n@@ -1 +1 @@\n-o\n+n\ndiff --git a/bin b/bin\nnew file mode 100644\nGIT binary patch\nliteral 1\nWc-qTI&B@7E0000000000\n").unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.current_dir(dir.path()).arg("split").arg("mixed.patch");

  cmd.assert().success();
  assert_eq!(
    fs::read_to_string(dir.path().join("text.trim.patch")).unwrap(),
    "file text\n\n-o\n+n\n"
  );
  assert_eq!(
    fs::read_to_string(dir.path().join("bin.trim.patch")).unwrap(),
    "file bin\n"
  );
}
