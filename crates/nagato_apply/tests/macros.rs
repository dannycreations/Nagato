#[macro_export]
macro_rules! parse_diff {
  ($diff:expr) => {{
    let diff = indoc::indoc!($diff);
    Parser::new(diff.as_bytes())
  }};
}

#[macro_export]
macro_rules! test_apply_ok {
  (
    $test_name:ident,
    diff: $diff:expr,
    source: $source:expr,
    expected: $expected:expr
  ) => {
    #[test]
    fn $test_name() {
      let patch = parse_diff!($diff).next().unwrap().unwrap();
      let mut output = Vec::new();
      apply(&mut output, &patch, $source.as_bytes()).unwrap();
      assert_eq!(String::from_utf8(output).unwrap(), $expected);
    }
  };
}

#[macro_export]
macro_rules! test_apply_err {
  (
    $test_name:ident,
    diff: $diff:expr,
    source: $source:expr
  ) => {
    #[test]
    fn $test_name() {
      let patch = parse_diff!($diff).next().unwrap().unwrap();
      let mut sink = sink();
      assert!(apply(&mut sink, &patch, $source.as_bytes()).is_err());
    }
  };
}

#[macro_export]
macro_rules! test_patch_ok {
  (
    $test_name:ident,
    $(reverse: $reverse:expr,)?
    initial_fs: { $($initial_path:expr => $initial_content:expr),* },
    diff: $diff:expr,
    assertions: |$root:ident| { $($assertions:tt)* }
  ) => {
    #[test]
    fn $test_name() {
      let dir = create_test_fs! { $($initial_path => $initial_content),* };
      let fs = FileSystem::new(dir.path(), false);
      let reverse = false $(|| $reverse)?;
      for patch in parse_diff!($diff) {
        patch_file(&fs, patch.unwrap(), reverse).unwrap();
      }
      let $root = dir.path();
      $($assertions)*
    }
  };
}

#[macro_export]
macro_rules! test_patch_err {
  (
    $test_name:ident,
    $(reverse: $reverse:expr,)?
    initial_fs: { $($path:expr => $content:expr),* },
    diff: $diff:expr
  ) => {
    #[test]
    fn $test_name() {
      let dir = create_test_fs! { $($path => $content),* };
      let fs = FileSystem::new(dir.path(), false);
      let patch = parse_diff!($diff).next().unwrap().unwrap();
      let reverse = false $(|| $reverse)?;
      assert!(patch_file(&fs, patch, reverse).is_err());
    }
  };
}

#[macro_export]
macro_rules! test_parser_err {
  (
    $test_name:ident,
    diff: $diff:expr,
    expected: $expected:expr
  ) => {
    #[test]
    fn $test_name() {
      let mut parser = parse_diff!($diff);
      let result = parser.next().unwrap();
      match result {
        Err(e) => {
          assert_eq!(e.kind, $expected);
        }
        Ok(_) => panic!("Expected an error but got Ok"),
      }
    }
  };
}

#[macro_export]
macro_rules! test_lexer_ok {
  (
    $test_name:ident,
    input: $input:expr,
    expected: [$($expected_token:expr),*]
  ) => {
    #[allow(unused_imports)]
    #[test]
    fn $test_name() {
      let input = indoc::indoc!($input);
      let tokens: Vec<_> = Lexer::new(input.as_bytes())
        .map(|r| r.unwrap().token)
        .collect();

      let expected = vec![$($expected_token),*];

      assert_eq!(tokens.len(), expected.len());
      for (got, exp) in tokens.into_iter().zip(expected.into_iter()) {
        match (got, exp) {
          // `---`/`+++` keep the "a/" or "b/" prefix the lexer saw, so
          // compare them after the parser's unquoting step.
          (TokenKind::OldFile(g), TokenKind::OldFile(e))
          | (TokenKind::NewFile(g), TokenKind::NewFile(e)) => {
            assert_eq!(unquote_path(g), unquote_path(e));
          }
          (g, e) => assert_eq!(g, e),
        }
      }
    }
  };
}

#[macro_export]
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
      let fs = FileSystem::new(dir.path(), false);
      let patch = parse_diff!($diff).next().unwrap().unwrap();
      let result = patch_file(&fs, patch, false);
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

#[macro_export]
macro_rules! test_delta_err {
  (
    $test_name:ident,
    delta: $delta:expr,
    source: $source:expr,
    expected: $expected:expr
  ) => {
    #[test]
    fn $test_name() {
      let mut output = Vec::new();
      let res = apply_delta(Cursor::new($delta), $source, &mut output);
      assert_eq!(res.unwrap_err().kind, $expected);
    }
  };
}
