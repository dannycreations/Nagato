use std::fs;

use tempfile::tempdir;

#[test]
fn cli_merge_multiple_files() {
  let dir = tempdir().unwrap();
  let p1 = dir.path().join("p1.patch");
  let p2 = dir.path().join("p2.patch");

  fs::write(&p1, "--- a/f1\n+++ b/f1\n@@ -1 +1 @@\n-1\n+A\n").unwrap();
  fs::write(&p2, "--- a/f2\n+++ b/f2\n@@ -1 +1 @@\n-2\n+B\n").unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd
    .current_dir(dir.path())
    .arg("merge")
    .arg("-o")
    .arg("out.patch")
    .arg("p1.patch")
    .arg("p2.patch");

  cmd.assert().success();

  let content = fs::read_to_string(dir.path().join("out.patch")).unwrap();
  assert!(content.contains("file f1"));
  assert!(content.contains("file f2"));
  assert!(content.find("file f1").unwrap() < content.find("file f2").unwrap());
}

#[test]
fn cli_merge_combine_same_file() {
  let dir = tempdir().unwrap();
  let p1 = dir.path().join("p1.patch");
  let p2 = dir.path().join("p2.patch");

  fs::write(&p1, "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-1\n+A\n").unwrap();
  fs::write(&p2, "--- a/f\n+++ b/f\n@@ -10 +10 @@\n-2\n+B\n").unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd
    .current_dir(dir.path())
    .arg("merge")
    .arg("-o")
    .arg("out.patch")
    .arg("p1.patch")
    .arg("p2.patch");

  cmd.assert().success();

  let content = fs::read_to_string(dir.path().join("out.patch")).unwrap();
  let occurrences: Vec<_> = content.matches("file f").collect();
  assert_eq!(occurrences.len(), 1);
  assert!(content.contains("-1\n+A"));
  assert!(content.contains("-2\n+B"));
}

#[test]
fn cli_merge_metadata_and_labels() {
  let dir = tempdir().unwrap();
  let p1 = dir.path().join("p1.patch");
  fs::write(
    &p1,
    "--- a/f\n+++ b/f\nlabel my_label\n@@ -1 +1 @@\n-a\n+b\n",
  )
  .unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd
    .current_dir(dir.path())
    .arg("merge")
    .arg("-o")
    .arg("out.patch")
    .arg("p1.patch");

  cmd.assert().success();
  let content = fs::read_to_string(dir.path().join("out.patch")).unwrap();
  assert_eq!(content, "file f\nlabel my_label\n\n-a\n+b\n");
}

#[test]
fn cli_merge_defaults() {
  let dir = tempdir().unwrap();
  fs::write(dir.path().join("p.patch"), "--- a/f\n+++ b/f\n").unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.current_dir(dir.path()).arg("merge").arg("p.patch");

  cmd.assert().success();
  assert!(dir.path().join("merge.patch").exists());
}
