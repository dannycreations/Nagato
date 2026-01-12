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
      let dir = tempfile::Builder::new().prefix("temp").tempdir().unwrap();
      $(
        let file_path = dir.path().join($path);
        if let Some(parent) = file_path.parent() {
          std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(file_path, $content).unwrap();
      )*

      let patch_file_path = dir.path().join(
        None.or(None $(.or(Some($patch_name)))?).unwrap_or("test.patch")
      );
      std::fs::write(&patch_file_path, indoc::indoc!($diff)).unwrap();

      let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato"));
      cmd.current_dir(dir.path());
      $(cmd.arg($arg);)*
      cmd.arg(patch_file_path.file_name().unwrap());

      cmd
        .assert()
        .success()
        .stdout(predicates::prelude::predicate::str::is_empty())
        .stderr(predicates::prelude::predicate::str::is_empty());

      assert_eq!(std::fs::read_to_string(dir.path().join($file_to_check)).unwrap(), $expected_content);
      $(assert_eq!(std::fs::read_to_string(dir.path().join($file_to_check2)).unwrap(), $expected_content2);)?
    }
  };
}
