#[macro_export]
macro_rules! test_get_line {
  (
    $test_name:ident,
    input: $input:expr,
    expected: $expected:expr
  ) => {
    #[test]
    fn $test_name() {
      assert_eq!(get_line($input), $expected);
    }
  };
}

#[macro_export]
macro_rules! test_parse_int {
  (
    $test_name:ident,
    type: $type:ty,
    input: $input:expr,
    radix: $radix:expr,
    expected: $expected:expr
  ) => {
    #[test]
    fn $test_name() {
      assert_eq!(parse_int::<$type>($input, $radix), $expected);
    }
  };
}

#[macro_export]
macro_rules! test_strip_prefix {
  (
    $test_name:ident,
    input: $input:expr,
    expected: $expected:expr
  ) => {
    #[test]
    fn $test_name() {
      assert_eq!(strip_diff_prefix($input), $expected);
    }
  };
}

#[macro_export]
macro_rules! test_unquote_path {
  (
    $test_name:ident,
    input: $input:expr,
    expected: $expected:expr
  ) => {
    #[test]
    fn $test_name() {
      assert_eq!(unquote_path($input).as_ref(), $expected);
    }
  };
}

#[macro_export]
macro_rules! test_fs_invalid_path {
  ($($name:ident => $input:expr),* $(,)?) => {
    $(
      #[test]
      fn $name() {
        let dir = create_test_fs! {};
        let fs = FileSystem::new(dir.path(), false);
        assert!(matches!(
          fs.read($input).unwrap_err().kind,
          ErrorKind::InvalidPath
        ));
      }
    )*
  };
}

#[macro_export]
macro_rules! test_fs_ops_ok {
  (
    $test_name:ident,
    check_mode: $check:expr,
    assertions: |$fs:ident, $dir:ident| { $($assertions:tt)* }
  ) => {
    #[test]
    fn $test_name() {
      let $dir = create_test_fs! {};
      let $fs = FileSystem::new($dir.path(), $check);
      $($assertions)*
    }
  };
}
