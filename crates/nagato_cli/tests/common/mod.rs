macro_rules! test_exec_ok {
  (
    $test_name:ident,
    initial_fs: { $($path:expr => $content:expr),* },
    diff: $diff:expr,
    args: [$($arg:expr),*],
    assert_file: ($file_to_check:expr, $expected_content:expr)
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

      let diff = indoc::indoc!($diff);
      let patch_file_path = dir.path().join("test.patch");
      std::fs::write(&patch_file_path, diff).unwrap();

      let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_nagato_cli"));
      cmd
        .current_dir(dir.path())
        .arg(patch_file_path.file_name().unwrap())
        $(.arg($arg))*;

      cmd
        .assert()
        .success()
        .stdout(predicates::prelude::predicate::str::is_empty())
        .stderr(predicates::prelude::predicate::str::is_empty());

      let final_file_path = dir.path().join($file_to_check);
      assert_eq!(std::fs::read_to_string(final_file_path).unwrap(), $expected_content);
    }
  };
}
