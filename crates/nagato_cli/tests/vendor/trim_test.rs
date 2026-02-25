use std::fs;

use tempfile::tempdir;

#[test]
fn cli_trim_basic() {
  let dir = tempdir().unwrap();
  let p = dir.path().join("test.patch");
  fs::write(&p, "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-a\n+b\n").unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.current_dir(dir.path()).arg("trim").arg("test.patch");

  cmd.assert().success();
  assert_eq!(
    fs::read_to_string(dir.path().join("test.trim.patch")).unwrap(),
    "file f\n\n-a\n+b\n"
  );
}

#[test]
fn cli_trim_with_label() {
  let dir = tempdir().unwrap();
  let p = dir.path().join("test.patch");
  fs::write(&p, "--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@ my_label\n-a\n+b\n")
    .unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.current_dir(dir.path()).arg("trim").arg("test.patch");

  cmd.assert().success();
  assert_eq!(
    fs::read_to_string(dir.path().join("test.trim.patch")).unwrap(),
    "file f\nlabel my_label\n\n-a\n+b\n"
  );
}

#[test]
fn cli_trim_multiple_in_one_file() {
  let dir = tempdir().unwrap();
  let p = dir.path().join("multi.patch");
  fs::write(&p, "--- a/f1\n+++ b/f1\n@@ -0,0 +1,1 @@\n+1\n--- a/f2\n+++ b/f2\n@@ -0,0 +1,1 @@\n+2\n").unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.current_dir(dir.path()).arg("trim").arg("multi.patch");

  cmd.assert().success();
  let content =
    fs::read_to_string(dir.path().join("multi.trim.patch")).unwrap();
  assert!(content.contains("file f1"));
  assert!(content.contains("file f2"));
}

#[test]
fn cli_trim_custom_directory() {
  let dir = tempdir().unwrap();
  let p = dir.path().join("test.patch");
  fs::write(&p, "--- a/f\n+++ b/f\n").unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd
    .current_dir(dir.path())
    .arg("trim")
    .arg("-d")
    .arg("out")
    .arg("test.patch");

  cmd.assert().success();
  assert!(dir.path().join("out/test.trim.patch").exists());
}

#[test]
fn cli_trim_stdin_and_malformed() {
  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.arg("trim").write_stdin("--- a/f\n+++ b/f\n");
  cmd.assert().success();

  let dir = tempdir().unwrap();
  let p = dir.path().join("invalid.patch");
  let mut content = b"--- a/".to_vec();
  content.extend_from_slice(&[0xFF, 0xFE]);
  content.extend_from_slice(b"\n+++ b/f\n@@ -1,1 +1,1 @@\n-a\n+b\n");
  fs::write(&p, content).unwrap();

  let mut cmd2 = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd2
    .current_dir(dir.path())
    .arg("trim")
    .arg("invalid.patch");
  cmd2.assert().success();
}
