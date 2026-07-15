#[macro_export]
macro_rules! test_exec_ok {
  (
    $test_name:ident,
    initial_fs: { $($path:expr => $content:expr),* },
    diff: $diff:expr,
    args: [$($arg:expr),*],
    $(patch_name: $patch_name:expr,)?
    assert_file: ($file_to_check:expr, $expected_content:expr)
    $(, assert_file2: ($file_to_check2:expr, $expected_content2:expr))?
  ) => {
    #[test]
    fn $test_name() {
      let dir = create_test_fs! { $($path => $content),* };

      let patch_file_path = dir.path().join(
        None.or(None $(.or(Some($patch_name)))?).unwrap_or("test.patch")
      );
      write(&patch_file_path, indoc::indoc!($diff)).unwrap();

      let mut cmd = Command::new(env!("CARGO_BIN_EXE_nagato"));
      cmd.current_dir(dir.path());
      $(cmd.arg($arg);)*
      cmd.arg(patch_file_path.file_name().unwrap());

      cmd
        .assert()
        .success()
        .stdout(is_empty())
        .stderr(is_empty());

      assert_eq!(read_to_string(dir.path().join($file_to_check)).unwrap(), $expected_content);
      $(assert_eq!(read_to_string(dir.path().join($file_to_check2)).unwrap(), $expected_content2);)?
    }
  };
}

#[macro_export]
macro_rules! test_cli_fail {
  (
    $test_name:ident,
    args: [$($arg:expr),*],
    $(stdin: $stdin:expr,)?
    stderr: $stderr:expr
  ) => {
    #[test]
    fn $test_name() {
      let mut cmd = Command::new(env!("CARGO_BIN_EXE_nagato"));
      $(cmd.arg($arg);)*
      $(cmd.write_stdin($stdin);)?
      cmd
        .assert()
        .failure()
        .stderr(contains($stderr));
    }
  };
}

#[macro_export]
macro_rules! test_cli_ok {
  (
    $test_name:ident,
    args: [$($arg:expr),*]
    $(, stdout_contains: $stdout:expr)?
    $(, stderr_contains: $stderr:expr)?
    $(,)?
  ) => {
    #[test]
    fn $test_name() {
      let mut cmd = Command::new(env!("CARGO_BIN_EXE_nagato"));
      $(cmd.arg($arg);)*
      let assert = cmd.assert().success();
      $(assert.stdout(contains($stdout));)?
      $(assert.stderr(contains($stderr));)?
    }
  };
}
