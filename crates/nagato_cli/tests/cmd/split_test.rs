test_exec_ok!(
  exec_split_command,
  initial_fs: {},
  diff: r#"
    --- a/foo.txt
    +++ b/foo.txt
    @@ -1,1 +1,1 @@
    -foo
    +FOO
    --- a/bar.txt
    +++ b/bar.txt
    @@ -1,1 +1,1 @@
    -bar
    +BAR
  "#,
  args: ["split"],
  patch_name: "multi.patch",
  assert_file: (
    "foo.txt.trim.patch",
    "file foo.txt\n\n-foo\n+FOO\n"
  ),
  assert_file2: (
    "bar.txt.trim.patch",
    "file bar.txt\n\n-bar\n+BAR\n"
  )
);

test_exec_ok!(
  exec_split_command_with_directory,
  initial_fs: {},
  diff: r#"
    --- a/foo.txt
    +++ b/foo.txt
    @@ -1,1 +1,1 @@
    -foo
    +FOO
  "#,
  args: ["split", "--directory", "out"],
  patch_name: "test.patch",
  assert_file: ("out/foo.txt.trim.patch", "file foo.txt\n\n-foo\n+FOO\n")
);

test_exec_ok!(
  exec_split_command_with_path_in_patch,
  initial_fs: {},
  diff: r#"
    --- a/src/foo.txt
    +++ b/src/foo.txt
    @@ -1,1 +1,1 @@
    -foo
    +FOO
  "#,
  args: ["split"],
  patch_name: "path.patch",
  assert_file: (
    "foo.txt.trim.patch",
    "file src/foo.txt\n\n-foo\n+FOO\n"
  )
);

#[test]
fn exec_split_command_conflict_resolution() {
  let dir = tempfile::Builder::new().prefix("temp").tempdir().unwrap();
  let patch_path = dir.path().join("test.patch");
  let existing_path = dir.path().join("foo.txt.trim.patch");
  let conflict_path = dir.path().join("foo-1.txt.trim.patch");

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
  cmd.arg("split").arg("test.patch");

  cmd
    .assert()
    .success()
    .stdout(predicates::prelude::predicate::str::is_empty())
    .stderr(predicates::prelude::predicate::str::is_empty());

  assert_eq!(std::fs::read_to_string(existing_path).unwrap(), "existing");
  assert_eq!(
    std::fs::read_to_string(conflict_path).unwrap(),
    "file foo.txt\n\n-foo\n+FOO\n"
  );
}
