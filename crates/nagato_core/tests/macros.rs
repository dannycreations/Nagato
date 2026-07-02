#[macro_export]
macro_rules! test_atomic_writer_ok {
  (
    $test_name:ident,
    content: $content:expr
  ) => {
    #[test]
    fn $test_name() {
      let dir = nagato_core::create_test_fs! {};
      let file_path = dir.path().join("test.txt");

      let mut writer = nagato_core::AtomicWriter::new(&file_path).unwrap();
      ::std::io::Write::write_all(&mut writer, $content).unwrap();
      writer.commit().unwrap();

      assert_eq!(std::fs::read(file_path).unwrap(), $content);
    }
  };
}

#[macro_export]
macro_rules! test_atomic_writer_err {
  (
    $test_name:ident,
    path: $path:expr
  ) => {
    #[test]
    fn $test_name() {
      assert!(
        nagato_core::AtomicWriter::new(std::path::Path::new($path)).is_err()
      );
    }
  };
}

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
macro_rules! test_fs_err {
  (
    $test_name:ident,
    fs: $fs:expr,
    method: $method:ident,
    arg: $arg:expr,
    expected: $expected:pat
  ) => {
    #[test]
    fn $test_name() {
      let res = $fs.$method($arg);
      assert!(matches!(res.unwrap_err().kind, $expected));
    }
  };
}

#[macro_export]
macro_rules! test_fs_invalid_path {
  ($($name:ident => $input:expr),* $(,)?) => {
    $(
      #[test]
      fn $name() {
        let dir = nagato_core::create_test_fs! {};
        let fs = nagato_core::FileSystem::new(dir.path(), false);
        assert!(matches!(
          fs.read($input).unwrap_err().kind,
          nagato_core::ErrorKind::InvalidPath
        ));
      }
    )*
  };
}

#[macro_export]
macro_rules! test_fs_get_unique_path_ok {
  (
    $test_name:ident,
    initial_fs: { $($path:expr => $content:expr),* },
    base_name: $base:expr,
    expected_file_name: $expected:expr
  ) => {
    #[test]
    fn $test_name() {
      let dir = nagato_core::create_test_fs! { $($path => $content),* };
      let res = nagato_core::get_unique_path(dir.path(), $base);
      assert_eq!(
        res.file_name().unwrap().to_str().unwrap(),
        $expected
      );
    }
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
      let $dir = nagato_core::create_test_fs! {};
      let $fs = nagato_core::FileSystem::new($dir.path(), $check);
      $($assertions)*
    }
  };
}

#[macro_export]
macro_rules! test_line_writer_ok {
  (
    $test_name:ident,
    assertions: |$writer:ident, $buf:ident| { $($assertions:tt)* }
  ) => {
    #[test]
    fn $test_name() {
      let mut $buf = Vec::new();
      let mut $writer = nagato_core::LineWriter::new(&mut $buf);
      $($assertions)*
    }
  };
}
