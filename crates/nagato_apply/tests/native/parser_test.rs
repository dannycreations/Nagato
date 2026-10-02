use nagato_apply::Parser;
use nagato_core::ErrorKind;

test_parser_err!(
  test_parser_error_no_file_info,
  diff: "@@ -1,1 +1,1 @@\n-old\n+new\n",
  expected: ErrorKind::PatchHasContentButNoFileInfo
);

test_parser_err!(
  test_parser_error_line_count_mismatch,
  diff: "--- a/file.txt\n+++ b/file.txt\n@@ -1,1 +1,2 @@\n-old\n+new\n",
  expected: ErrorKind::HunkLineCountMismatch
);

#[test]
fn test_parser_index_mode() {
  let patch = parse_diff!(
    "diff --git a/file b/file\nindex abcdef0..1234567 100755\n--- a/file\n+++ b/file\n"
  )
  .next()
  .unwrap()
  .unwrap();
  assert_eq!(patch.new_mode, Some(0o100755));
}

#[test]
fn test_parser_hunk_range_defaults() {
  let patch =
    parse_diff!("--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n")
      .next()
      .unwrap()
      .unwrap();
  assert_eq!(patch.hunks[0].old_span, 1);
  assert_eq!(patch.hunks[0].new_span, 1);
}

#[test]
fn test_parser_binary_files_header_paths() {
  // The header names one path per side, separated by "and". A header that
  // carries a single path names that path on both sides.
  let patch = parse_diff!("Binary files a/old.bin and b/new.bin differ\n")
    .next()
    .unwrap()
    .unwrap();
  assert_eq!(patch.old_file.as_ref(), b"old.bin");
  assert_eq!(patch.new_file.as_ref(), b"new.bin");
  assert!(patch.binary);

  let patch = parse_diff!("Binary files a/only.bin differ\n")
    .next()
    .unwrap()
    .unwrap();
  assert_eq!(patch.old_file.as_ref(), b"only.bin");
  assert_eq!(patch.new_file.as_ref(), b"only.bin");
}

test_parser_err!(
  test_parser_error_invalid_hunk_range_old_non_digit,
  diff: "--- a/file.txt\n+++ b/file.txt\n@@ -a,1 +1,1 @@\n-old\n+new\n",
  expected: ErrorKind::InvalidHunkRange
);

test_parser_err!(
  test_parser_error_invalid_hunk_range_new_non_digit,
  diff: "--- a/file.txt\n+++ b/file.txt\n@@ -1,1 +1,a @@\n-old\n+new\n",
  expected: ErrorKind::InvalidHunkRange
);

test_parser_err!(
  test_parser_error_invalid_hunk_range_no_comma_non_digit,
  diff: "--- a/file.txt\n+++ b/file.txt\n@@ -a +1 @@\n-old\n+new\n",
  expected: ErrorKind::InvalidHunkRange
);
