use nagato_core::ErrorKind;

test_patch_err_with_line!(
  fails_finding_hunk_context,
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
  fails_reporting_error_on_best_match,
  initial_fs: {
    "a.txt" => "\n// Section 1\nA\nB\n\n// Section 2\nA\nC\nD\n"
  },
  diff: r#"
    file a/a.txt

    -A
    -C
    -*error*E
  "#,
  expected_line: 5,
  expected_kind: ErrorKind::CouldNotApplyHunk
);

test_patch_err_with_line!(
  fails_matching_context_after_empty_line,
  initial_fs: { "a.txt" => "A\n\nB\n" },
  diff: r#"
    file a.txt
     A

     C
  "#,
  expected_line: 4,
  expected_kind: ErrorKind::CouldNotApplyHunk
);

test_patch_err!(
  fails_without_hunkless_heuristic,
  initial_fs: { "file.txt" => "actual line 1\nactual line 2\nold line\n" },
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
     wrong context 1
     actual line 2
    -old line
    +new line
  "#
);
