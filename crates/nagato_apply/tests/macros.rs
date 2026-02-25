#[macro_export]
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

#[macro_export]
macro_rules! parse_diff {
  ($diff:expr) => {{
    let diff = indoc::indoc!($diff);
    nagato_apply::Parser::new(diff.as_bytes())
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
      nagato_apply::apply(&mut output, &patch, $source.as_bytes()).unwrap();
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
      let mut sink = std::io::sink();
      assert!(
        nagato_apply::apply(&mut sink, &patch, $source.as_bytes()).is_err()
      );
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
      let mut fs = nagato_core::FileSystem::new(dir.path(), false);
      let reverse = false $(|| $reverse)?;
      for patch in parse_diff!($diff) {
        nagato_apply::patch_file(&mut fs, patch.unwrap(), reverse).unwrap();
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
      let mut fs = nagato_core::FileSystem::new(dir.path(), false);
      let patch = parse_diff!($diff).next().unwrap().unwrap();
      let reverse = false $(|| $reverse)?;
      assert!(nagato_apply::patch_file(&mut fs, patch, reverse).is_err());
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
      let mut fs = nagato_core::FileSystem::new(dir.path(), false);
      let patch = parse_diff!($diff).next().unwrap().unwrap();
      let result = nagato_apply::patch_file(&mut fs, patch, false);
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
macro_rules! test_patch_invert {
  (
    $test_name:ident,
    patch: {
      old_file: $old:expr,
      new_file: $new:expr
      $(, rename_from: $from:expr)?
      $(, rename_to: $to:expr)?
      $(,)?
    },
    expected: {
      old_file: $e_old:expr
      $(, rename_from: $e_from:expr)?
      $(,)?
    }
  ) => {
    #[test]
    fn $test_name() {
      use std::borrow::Cow;
      use nagato_apply::Patch;
      let patch = Patch {
        old_file: Cow::Borrowed($old),
        new_file: Cow::Borrowed($new),
        $(rename_from: Some(Cow::Borrowed($from as &[u8])),)?
        $(rename_to: Some(Cow::Borrowed($to as &[u8])),)?
        ..Default::default()
      };
      let inverted = patch.invert();
      assert_eq!(inverted.old_file.as_ref(), $e_old);
      $(assert_eq!(inverted.rename_from, Some(Cow::Borrowed($e_from as &[u8])));)?
    }
  };
}

#[macro_export]
macro_rules! test_parser_ok {
  (
    $test_name:ident,
    $diff:expr,
    assertions: |$patch:ident| { $($assertions:tt)* }
  ) => {
    #[test]
    fn $test_name() {
      let mut parser = parse_diff!($diff);
      let $patch = parser.next().unwrap().unwrap();
      $($assertions)*
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
      use std::io::Cursor;
      let mut output = Vec::new();
      let res =
        nagato_apply::apply_delta(Cursor::new($delta), $source, &mut output);
      assert_eq!(res.unwrap_err().kind, $expected);
    }
  };
}

#[macro_export]
macro_rules! test_reject_mixed {
  (
    $test_name:ident,
    initial_fs: { $initial_path:expr => $initial_content:expr },
    patch: $patch:expr
  ) => {
    #[test]
    fn $test_name() {
      let dir = create_test_fs! { $initial_path => $initial_content };
      let fs = nagato_core::FileSystem::new(dir.path(), false);
      let res = nagato_apply::patch_file(&fs, $patch, false);
      assert_eq!(
        res.unwrap_err().kind,
        nagato_core::ErrorKind::UnsupportedBinaryPatch
      );
    }
  };
}

#[macro_export]
macro_rules! test_lexer_binary_data_ok {
  (
    $test_name:ident,
    input: $input:expr,
    expected: [$($expected_data:expr),*]
  ) => {
    #[test]
    fn $test_name() {
      use nagato_apply::{Lexer, LexerMode, TokenKind};
      let mut lexer = Lexer::new($input);
      lexer.set_mode(LexerMode::Binary);
      $(
        assert!(matches!(
          lexer.next().unwrap().unwrap().token,
          TokenKind::BinaryData($expected_data)
        ));
      )*
    }
  };
}

#[macro_export]
macro_rules! test_applier_flush_ok {
  (
    $test_name:ident,
    source: $source:expr,
    patch: $patch:expr,
    expected_contains: $expected:expr
  ) => {
    #[test]
    fn $test_name() {
      use nagato_apply::Applier;
      let mut output = Vec::new();
      let applier = Applier::new(&mut output, $source);
      applier.process(&$patch).unwrap();
      assert!(String::from_utf8_lossy(&output).contains($expected));
    }
  };
}

#[macro_export]
macro_rules! test_binary_applier_process_ok {
  (
    $test_name:ident,
    source: $source:expr,
    patch: $patch:expr
  ) => {
    #[test]
    fn $test_name() {
      use nagato_apply::Applier;
      let mut output = Vec::new();
      let mut applier = Applier::new(&mut output, $source);
      let _ = applier.process_binary(&$patch);
    }
  };
}
