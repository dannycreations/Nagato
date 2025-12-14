use nagato_core::error::ErrorKind;

test_patch_err_with_line!(
  fails_with_correct_line_number_on_hunk_apply_error,
  initial_fs: {},
  diff: r#"
        --- a/a.txt
        +++ b/a.txt
        @@ -1,1 +1,1 @@
        -hello
        +world
    "#,
  expected_line: 3,
  expected_kind: ErrorKind::CouldNotApplyHunk
);

test_patch_err_with_line!(
  fails_with_correct_line_number_for_headerless_hunk,
  initial_fs: { "a.txt" => "hello\n" },
  diff: r#"
        --- a/a.txt
        +++ b/a.txt
        -goodbye
        +world
    "#,
  expected_line: 3,
  expected_kind: ErrorKind::CouldNotApplyHunk
);

test_patch_err_with_line!(
  fails_with_correct_line_number_for_shortest_header,
  initial_fs: { "a.txt" => "hello\n" },
  diff: r#"
        a/a.txt
        -goodbye
        +world
    "#,
  expected_line: 2,
  expected_kind: ErrorKind::CouldNotApplyHunk
);
