test_exec_ok!(
  exec_merge_command,
  initial_fs: {},
  diff: r#"
    --- a/foo.txt
    +++ b/foo.txt
    @@ -1,1 +1,1 @@
    -foo
    +FOO
  "#,
  args: ["merge", "--output", "merge.patch"],
  patch_name: "patch1.patch",
  assert_file: (
    "merge.patch",
    "file foo.txt\n\n-foo\n+FOO\n"
  )
);

#[test]
fn exec_merge_command_multiple_files() {
  let dir = tempfile::Builder::new().prefix("temp").tempdir().unwrap();
  let p1 = dir.path().join("p1.patch");
  let p2 = dir.path().join("p2.patch");
  let out = dir.path().join("merge.patch");

  std::fs::write(
    &p1,
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

  std::fs::write(
    &p2,
    indoc::indoc!(
      r#"
    --- a/bar.txt
    +++ b/bar.txt
    @@ -1,1 +1,1 @@
    -bar
    +BAR
  "#
    ),
  )
  .unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.current_dir(dir.path());
  cmd
    .arg("merge")
    .arg("--output")
    .arg("merge.patch")
    .arg("p1.patch")
    .arg("p2.patch");

  cmd.assert().success();

  let content = std::fs::read_to_string(out).unwrap();
  assert!(content.contains("file foo.txt"));
  assert!(content.contains("file bar.txt"));
  assert!(content.contains("-foo\n+FOO"));
  assert!(content.contains("-bar\n+BAR"));
}

#[test]
fn exec_merge_command_combine_same_file() {
  let dir = tempfile::Builder::new().prefix("temp").tempdir().unwrap();
  let p1 = dir.path().join("p1.patch");
  let p2 = dir.path().join("p2.patch");
  let out = dir.path().join("merge.patch");

  std::fs::write(
    &p1,
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

  std::fs::write(
    &p2,
    indoc::indoc!(
      r#"
    --- a/foo.txt
    +++ b/foo.txt
    @@ -5,1 +5,1 @@
    -bar
    +BAR
  "#
    ),
  )
  .unwrap();

  let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
  cmd.current_dir(dir.path());
  cmd
    .arg("merge")
    .arg("--output")
    .arg("merge.patch")
    .arg("p1.patch")
    .arg("p2.patch");

  cmd.assert().success();

  let content = std::fs::read_to_string(out).unwrap();
  // Should only have one "file foo.txt" header
  let occurrences: Vec<_> = content.matches("file foo.txt").collect();
  assert_eq!(occurrences.len(), 1);
  assert!(content.contains("-foo\n+FOO"));
  assert!(content.contains("-bar\n+BAR"));
}
