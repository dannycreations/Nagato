macro_rules! create_test_fs {
  { $($path:expr => $content:expr),* } => {
    {
      let dir = tempfile::Builder::new()
        .prefix("test")
        .tempdir()
        .unwrap();
      $(
        let file_path = dir.path().join($path);
        if let Some(parent) = file_path.parent() {
          std::fs::create_dir_all(parent).unwrap();
        }
        let content: &[u8] = $content.as_ref();
        std::fs::write(file_path, content).unwrap();
      )*
      dir
    }
  };
}

macro_rules! parse_diff {
  ($diff:expr) => {{
    let diff = indoc::indoc!($diff);
    nagato_apply::Parser::new(diff.as_bytes())
  }};
}

macro_rules! test_apply_ok {
  (
    $test_name:ident,
    $diff:expr,
    $source:expr,
    $expected:expr
  ) => {
    #[test]
    fn $test_name() {
      let patch = parse_diff!($diff).next().unwrap().unwrap();
      let mut output = Vec::new();
      nagato_apply::apply(&mut output, &patch, $source.as_bytes()).unwrap();
      assert_eq!(String::from_utf8(output).unwrap(), $expected);
    }
  };
}

macro_rules! test_apply_err {
  (
    $test_name:ident,
    $diff:expr,
    $source:expr
  ) => {
    #[test]
    fn $test_name() {
      let patch = parse_diff!($diff).next().unwrap().unwrap();
      let mut sink = std::io::sink();
      assert!(
        nagato_apply::apply(&mut sink, &patch, $source.as_bytes()).is_err()
      );
    }
  };
}

macro_rules! test_patch_ok {
  (
    $test_name:ident,
    reverse: $reverse:expr,
    initial_fs: { $($initial_path:expr => $initial_content:expr),* },
    diff: $diff:expr,
    assertions: |$root:ident| { $($assertions:tt)* }
  ) => {
    #[test]
    fn $test_name() {
      let dir = create_test_fs! { $($initial_path => $initial_content),* };
      let mut fs = nagato_core::FileSystem::new(dir.path());
      for patch in parse_diff!($diff) {
        nagato_apply::patch_file(&mut fs, patch.unwrap(), $reverse, false)
          .unwrap();
      }

      let $root = dir.path();
      $($assertions)*
    }
  };
  (
    $test_name:ident,
    initial_fs: { $($initial_path:expr => $initial_content:expr),* },
    diff: $diff:expr,
    assertions: |$root:ident| { $($assertions:tt)* }
  ) => {
    test_patch_ok!(
      $test_name,
      reverse: false,
      initial_fs: { $($initial_path => $initial_content),* },
      diff: $diff,
      assertions: |$root| { $($assertions)* }
    );
  };
}

macro_rules! test_patch_err {
  (
    $test_name:ident,
    reverse: $reverse:expr,
    initial_fs: { $($path:expr => $content:expr),* },
    diff: $diff:expr
  ) => {
    #[test]
    fn $test_name() {
      let dir = create_test_fs! { $($path => $content),* };
      let mut fs = nagato_core::FileSystem::new(dir.path());
      let mut parser = parse_diff!($diff);
      assert!(nagato_apply::patch_file(
        &mut fs,
        parser.next().unwrap().unwrap(),
        $reverse,
        false
      )
      .is_err());
    }
  };
  (
    $test_name:ident,
    initial_fs: { $($path:expr => $content:expr),* },
    diff: $diff:expr
  ) => {
    test_patch_err!(
      $test_name,
      reverse: false,
      initial_fs: { $($path => $content),* },
      diff: $diff
    );
  };
}

macro_rules! test_parser_err {
  (
    $test_name:ident,
    $diff:expr,
    $expected_kind:expr
  ) => {
    #[test]
    fn $test_name() {
      let mut parser = parse_diff!($diff);
      let result = parser.next().unwrap();
      match result {
        Err(e) => {
          assert_eq!(e.kind, $expected_kind);
        }
        Ok(_) => panic!("Expected an error but got Ok"),
      }
    }
  };
}

macro_rules! test_lexer_ok {
  (
    $test_name:ident,
    $input:expr,
    $($expected_token:expr),*
  ) => {
    #[test]
    fn $test_name() {
      let input = indoc::indoc!($input);
      let tokens: Vec<_> = nagato_apply::Lexer::new(input.as_bytes())
        .map(|r| r.unwrap().token)
        .collect();
      assert_eq!(tokens, vec![$($expected_token),*]);
    }
  };
}

macro_rules! test_patch_err_with_line {
  (
    $test_name:ident,
    initial_fs: { $($path:expr => $content:expr),* },
    diff: $diff:expr,
    expected_line: $expected_line:expr,
    expected_kind: $expected_kind:expr
  ) => {
    #[test]
    fn $test_name() {
      let dir = create_test_fs! { $($path => $content),* };
      let mut fs = nagato_core::FileSystem::new(dir.path());
      let patch = parse_diff!($diff).next().unwrap().unwrap();
      let result = nagato_apply::patch_file(&mut fs, patch, false, false);
      match result {
        Err(e) => {
          assert_eq!(e.line, Some($expected_line));
          assert_eq!(e.kind, $expected_kind);
        }
        Ok(_) => panic!("Expected an error but got Ok"),
      }
    }
  };
}
