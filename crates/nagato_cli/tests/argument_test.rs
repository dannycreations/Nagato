use std::fs;

use assert_cmd::Command;
use indoc::indoc;
use predicates::prelude::predicate::str as PStr;
use tempfile::Builder;

#[test]
fn exec_directory_argument() {
  let dir = Builder::new().prefix("temp").tempdir().unwrap();
  let project_dir = dir.path().join("project");
  fs::create_dir(&project_dir).unwrap();
  let file_to_patch = project_dir.join("file.txt");
  fs::write(&file_to_patch, "hello\n").unwrap();

  let diff = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        -hello
        +world
    "#};
  let patch_file_path = dir.path().join("test.patch");
  fs::write(&patch_file_path, diff).unwrap();

  let mut cmd = Command::new(env!("CARGO_BIN_EXE_nagato_cli"));
  cmd
    .current_dir(dir.path())
    .arg(patch_file_path.file_name().unwrap())
    .arg("--directory")
    .arg(project_dir.file_name().unwrap());

  cmd
    .assert()
    .success()
    .stdout(PStr::is_empty())
    .stderr(PStr::is_empty());

  assert_eq!(fs::read_to_string(file_to_patch).unwrap(), "world\n");
}

#[test]
fn exec_reverse_argument() {
  let dir = Builder::new().prefix("temp").tempdir().unwrap();
  let file_to_patch = dir.path().join("file.txt");
  fs::write(&file_to_patch, "world\n").unwrap();

  let diff = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        -hello
        +world
    "#};
  let patch_file_path = dir.path().join("test.patch");
  fs::write(&patch_file_path, diff).unwrap();

  let mut cmd = Command::new(env!("CARGO_BIN_EXE_nagato_cli"));
  cmd
    .current_dir(dir.path())
    .arg(patch_file_path.file_name().unwrap())
    .arg("--reverse");

  cmd
    .assert()
    .success()
    .stdout(PStr::is_empty())
    .stderr(PStr::is_empty());

  assert_eq!(fs::read_to_string(file_to_patch).unwrap(), "hello\n");
}
