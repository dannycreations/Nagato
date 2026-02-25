use std::fs;

use nagato_apply::{BinaryFragment, BinaryKind, Hunk, Line, LineKind, Patch};

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

test_patch_invert!(
  test_applier_invert,
  patch: {
    old_file: b"a/file",
    new_file: b"b/file",
    rename_from: b"old",
    rename_to: b"new",
  },
  expected: {
    old_file: b"b/file",
    rename_from: b"new",
  }
);

test_applier_flush_ok!(
  applier_flush_remaining,
  source: b"line1\nline2\n",
  patch: Patch {
    hunks: vec![Hunk {
      old_line: 1,
      old_span: 1,
      new_line: 1,
      new_span: 1,
      lines: vec![Line {
        kind: LineKind::Context,
        text: b"line1",
      }]
      .into_boxed_slice(),
      has_header: true,
      ..Default::default()
    }]
    .into_boxed_slice(),
    ..Default::default()
  },
  expected_contains: "line2"
);

test_reject_mixed!(
  test_rejects_mixed_binary_and_hunks,
  initial_fs: { "file.txt" => "content\n" },
  patch: Patch {
    binary: true,
    binary_fragments: vec![BinaryFragment {
      kind: BinaryKind::Literal,
      size: 1,
      data: vec![b"Wc-qT"],
    }]
    .into_boxed_slice(),
    hunks: vec![Hunk::default()].into_boxed_slice(),
    ..Default::default()
  }
);
