test_exec_ok!(
  exec_trim_command,
  initial_fs: {},
  diff: r#"
    diff --git a/foo.txt b/foo.txt
    index 1234567..89abcde 100644
    --- a/foo.txt
    +++ b/foo.txt
    @@ -1,3 +1,3 @@
     context
    -old
    +new
     context
  "#,
  args: ["trim"],
  patch_name: "test.patch",
  assert_file: ("test.trim.patch", "file foo.txt\n\n context\n-old\n+new\n context\n")
);

test_exec_ok!(
  exec_trim_command_with_label,
  initial_fs: {},
  diff: r#"
    --- a/bar.txt
    +++ b/bar.txt
    @@ -1,1 +1,1 @@ my_label
    -delete
    +add
  "#,
  args: ["trim"],
  patch_name: "test_label.patch",
  assert_file: ("test_label.trim.patch", "file bar.txt\nlabel my_label\n\n-delete\n+add\n")
);

test_exec_ok!(
  exec_trim_command_with_directory,
  initial_fs: {},
  diff: r#"
    --- a/foo.txt
    +++ b/foo.txt
    @@ -1,1 +1,1 @@
    -foo
    +FOO
  "#,
  args: ["trim", "--directory", "out"],
  patch_name: "test.patch",
  assert_file: ("out/test.trim.patch", "file foo.txt\n\n-foo\n+FOO\n")
);

#[test]
fn exec_trim_command_conflict_resolution() {
  let dir = tempfile::Builder::new().prefix("temp").tempdir().unwrap();
  let patch_path = dir.path().join("test.patch");
  let existing_path = dir.path().join("test.trim.patch");
  let _conflict_path = dir.path().join("test-1.trim.patch");

  std::fs::write(&existing_path, "existing").unwrap();
  std::fs::write(
    &patch_path,
    indoc::indoc!(
      r#"
    --- a/foo.txt
    +++ b/foo.txt
    @@ -1,1 +1,1 @@
    -foo
    +FOO
  "#
    ),
  )
  .unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.current_dir(dir.path());
  cmd.arg("trim").arg("test.patch");

  cmd
    .assert()
    .success()
    .stdout(predicates::prelude::predicate::str::is_empty())
    .stderr(predicates::prelude::predicate::str::is_empty());

  assert_eq!(std::fs::read_to_string(existing_path).unwrap(), "existing");
  assert_eq!(
    std::fs::read_to_string(dir.path().join("test-1.trim.patch")).unwrap(),
    "file foo.txt\n\n-foo\n+FOO\n"
  );
}
