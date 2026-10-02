use std::{fs, io::sink};

use nagato_apply::{
  apply, apply_to_fs, patch_file, Applier, BinaryFragment, BinaryKind, Hunk,
  Line, LineKind, Parser, Patch,
};
use nagato_core::{
  create_test_fs, strip_diff_prefix, unquote_path, ErrorKind, FileSystem,
};

test_patch_ok!(
  applier_matches_whitespace,
  initial_fs: { "file.txt" => " context line\n  deletion line\n" },
  diff: r#"
    diff --git a/file.txt b/file.txt
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
      context line
    -  deletion line
    +  addition line
  "#,
  assertions: |root| {
    let content = fs::read_to_string(root.join("file.txt")).unwrap();
    assert_eq!(content, " context line\n  addition line\n");
  }
);

test_patch_ok!(
  applier_patch_with_offset_line_numbers,
  initial_fs: { "file.txt" => "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\n some context\n some more context\n a final bit of context\nthe line to remove\n and more context\n and more context\n and a final context\nline 15\n" },
  diff: r#"
    diff --git a/file.txt b/file.txt
    --- a/file.txt
    +++ b/file.txt
    @@ -8,7 +8,7 @@
      some context
      some more context
      a final bit of context
    -the line to remove
    +the new line to add
      and more context
      and more context
      and a final context
  "#,
  assertions: |root| {
    let content = fs::read_to_string(root.join("file.txt")).unwrap();
    assert_eq!(content, "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\n some context\n some more context\n a final bit of context\nthe new line to add\n and more context\n and more context\n and a final context\nline 15\n");
  }
);

test_apply_ok!(
  applier_simple_patch,
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,3 +1,3 @@
     context 1
    -old line
    +new line
     context 2
  "#,
  source: "context 1\nold line\ncontext 2\n",
  expected: "context 1\nnew line\ncontext 2\n"
);

test_apply_ok!(
  applier_handles_newlines_at_eof,
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
    -line1
    -line2
    +Line1_Changed
    +line2
    \ No newline at end of file
  "#,
  source: "line1\nline2\n",
  expected: "Line1_Changed\nline2"
);

test_apply_ok!(
  applier_adds_trailing_newline,
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,1 +1,2 @@
    -hello
    +hello
    +world
  "#,
  source: "hello",
  expected: "hello\nworld\n"
);

test_apply_err!(
  applier_fails_on_context_mismatch,
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,1 +1,1 @@
     context expected line
  "#,
  source: "different line"
);

test_patch_ok!(
  applier_creates_and_deletes_file,
  initial_fs: { "to_delete.txt" => "line 1\nline 2\n" },
  diff: r#"
    diff --git a/new_file.txt b/new_file.txt
    new file mode 100644
    --- /dev/null
    +++ b/new_file.txt
    @@ -0,0 +1,1 @@
    +new line
    diff --git a/to_delete.txt b/to_delete.txt
    --- a/to_delete.txt
    +++ /dev/null
    @@ -1,2 +0,0 @@
    -line 1
    -line 2
  "#,
  assertions: |root| {
    assert!(root.join("new_file.txt").exists());
    assert!(!root.join("to_delete.txt").exists());
  }
);

test_patch_ok!(
  applier_renames_and_copies,
  initial_fs: { "old_name.txt" => "file content\n", "old_file.txt" => "content" },
  diff: r#"
    diff --git a/old_name.txt b/new_name.txt
    rename from old_name.txt
    rename to new_name.txt
    --- a/old_name.txt
    +++ b/new_name.txt
    @@ -1 +1 @@
    -file content
    +new content
    diff --git a/old_file.txt b/new_file.txt
    copy from old_file.txt
    copy to new_file.txt
  "#,
  assertions: |root| {
    assert!(!root.join("old_name.txt").exists());
    assert!(root.join("new_name.txt").exists());
    assert!(root.join("old_file.txt").exists());
    assert!(root.join("new_file.txt").exists());
  }
);

test_apply_ok!(
  applier_handles_empty_lines,
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,5 +1,5 @@
     line 1
 
     line 3
    -line 4
    +new line 4
     line 5
  "#,
  source: "line 1\n\nline 3\nline 4\nline 5\n",
  expected: "line 1\n\nline 3\nnew line 4\nline 5\n"
);

test_patch_err!(
  applier_fails_on_whitespace_mismatch,
  initial_fs: { "file.txt" => "    context\n" },
  diff: r#"
    --- file.txt
    +++ file.txt
    @@ -1,1 +1,1 @@
      context
  "#
);

#[test]
fn test_applier_invert() {
  let patch = Patch {
    old_file: unquote_path(b"a/file"),
    new_file: unquote_path(b"b/file"),
    rename_from: Some(unquote_path(b"old")),
    rename_to: Some(unquote_path(b"new")),
    ..Default::default()
  };
  let inverted = patch.invert();
  assert_eq!(inverted.old_file.as_ref(), strip_diff_prefix(b"b/file"));
  assert_eq!(inverted.rename_from, Some(unquote_path(b"new")));
}

#[test]
fn applier_flush_remaining() {
  let patch = Patch {
    lines: vec![Line {
      kind: LineKind::Context,
      text: b"line1",
    }],
    hunks: vec![Hunk {
      old_span: 1,
      new_span: 1,
      lines_start: 0,
      lines_len: 1,
      has_header: true,
      ..Default::default()
    }],
    ..Default::default()
  };

  let mut output = Vec::new();
  Applier::new(&mut output, b"line1\nline2\n")
    .process(&patch)
    .unwrap();
  assert!(String::from_utf8_lossy(&output).contains("line2"));
}

#[test]
fn test_rejects_mixed_binary_and_hunks() {
  let dir = create_test_fs! { "file.txt" => "content\n" };
  let fs = FileSystem::new(dir.path(), false);

  let patch = Patch {
    binary: true,
    binary_lines: vec![b"Wc-qT"],
    binary_fragments: vec![BinaryFragment {
      kind: BinaryKind::Literal,
      size: 1,
      data_start: 0,
      data_len: 1,
    }],
    hunks: vec![Hunk::default()],
    ..Default::default()
  };
  let res = patch_file(&fs, patch, false);
  assert_eq!(res.unwrap_err().kind, ErrorKind::UnsupportedBinaryPatch);
}

test_apply_ok!(
  applier_best_match_anchor,
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,3 +1,3 @@
     }
    -short
    +new_short
     a very long and unique context line to serve as anchor
   "#,
  source: "}\nother\n}\nshort\na very long and unique context line to serve as anchor\n",
  expected: "}\nother\n}\nnew_short\na very long and unique context line to serve as anchor\n"
);

test_apply_ok!(
  applier_hunkless_sequential_duplicates,
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    -item
    +item_modified1

    -item
    +item_modified2
  "#,
  source: "item\nitem\n",
  expected: "item_modified1\nitem_modified2\n"
);

test_apply_err!(
  applier_hunkless_fails_on_mismatch,
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    -expected line
    +new line
  "#,
  source: "different line"
);

test_apply_ok!(
  applier_hunkless_empty_source,
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    +item1

    +item2
  "#,
  source: "",
  expected: "item1\nitem2\n"
);

#[test]
fn applier_streamed_applies_hunks_in_document_order() {
  // Only the first hunk of a streamed patch decides whether the rest are
  // planned as a batch. The last hunk here can only match before the position
  // the previous one reached, so it must fail instead of being reordered ahead
  // of it, and the target must survive untouched.
  let dir = create_test_fs! { "file.txt" => "head\naaa\nbbb\n" };
  let fs = FileSystem::new(dir.path(), false);
  let patch = b"--- a/file.txt\n+++ b/file.txt\n@@ -1,1 +1,1 @@\n-head\n+HEAD\nlabel two\n-bbb\n+BBB\nlabel three\n-aaa\n+AAA\n";

  let res = apply_to_fs(&fs, patch, false);

  assert_eq!(res.unwrap_err().kind, ErrorKind::CouldNotApplyHunk);
  assert_eq!(
    fs::read_to_string(dir.path().join("file.txt")).unwrap(),
    "head\naaa\nbbb\n"
  );
}
