use std::fs;

use nagato_core::ErrorKind;

test_patch_ok!(
  matches_whitespace,
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
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      " context line\n  addition line\n"
    );
  }
);

test_patch_ok!(
  applies_patch_with_offset_line_numbers,
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
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\n some context\n some more context\n a final bit of context\nthe new line to add\n and more context\n and more context\n and a final context\nline 15\n"
    );
  }
);

test_apply_ok!(
  applies_simple_patch,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,3 +1,3 @@
     context 1
    -old line
    +new line
     context 2
  "#,
  "context 1\nold line\ncontext 2\n",
  "context 1\nnew line\ncontext 2\n"
);

test_apply_ok!(
  removes_trailing_newline,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
    -line1
    -line2
    +Line1_Changed
    +line2
    \ No newline at end of file
  "#,
  "line1\nline2\n",
  "Line1_Changed\nline2"
);

test_apply_ok!(
  adds_trailing_newline,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,1 +1,2 @@
    -hello
    +hello
    +world
  "#,
  "hello",
  "hello\nworld\n"
);

test_apply_ok!(
  preserves_and_adds_trailing_newline,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,3 @@
    -line1
    -line2
    \ No newline at end of file
    +line1
    +line2
    +line3
  "#,
  "line1\nline2",
  "line1\nline2\nline3\n"
);

test_apply_err!(
  fails_on_context_mismatch,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,1 +1,1 @@
     context expected line
  "#,
  "different line"
);

test_patch_ok!(
  creates_file,
  initial_fs: {},
  diff: r#"
    diff --git a/new_file.txt b/new_file.txt
    new file mode 100644
    index 0000000..abcdef0
    --- /dev/null
    +++ b/new_file.txt
    @@ -0,0 +1,2 @@
    +line 1
    +line 2
  "#,
  assertions: |root| {
    assert!(root.join("new_file.txt").exists());
    assert_eq!(
      fs::read_to_string(root.join("new_file.txt")).unwrap(),
      "line 1\nline 2\n"
    );
  }
);

test_patch_ok!(
  deletes_file,
  initial_fs: { "file_to_delete.txt" => "line 1\nline 2\n" },
  diff: r#"
    diff --git a/file_to_delete.txt b/file_to_delete.txt
    index abcdef0..0000000
    --- a/file_to_delete.txt
    +++ /dev/null
    @@ -1,2 +0,0 @@
    -line 1
    -line 2
  "#,
  assertions: |root| {
    assert!(!root.join("file_to_delete.txt").exists());
  }
);

test_patch_ok!(
  renames_file_with_content_change,
  initial_fs: { "old_name.txt" => "file content\n" },
  diff: r#"
    diff --git a/old_name.txt b/new_name.txt
    similarity index 80%
    rename from old_name.txt
    rename to new_name.txt
    --- a/old_name.txt
    +++ b/new_name.txt
    @@ -1 +1 @@
    -file content
    +new file content
  "#,
  assertions: |root| {
    assert!(!root.join("old_name.txt").exists());
    assert!(root.join("new_name.txt").exists());
    assert_eq!(
      fs::read_to_string(root.join("new_name.txt")).unwrap(),
      "new file content\n"
    );
  }
);

test_patch_ok!(
  renames_file_with_metadata_only,
  initial_fs: { "old_metadata.txt" => "content" },
  diff: r#"
    diff --git a/old_metadata.txt b/new_metadata.txt
    similarity index 100%
    rename from old_metadata.txt
    rename to new_metadata.txt
  "#,
  assertions: |root| {
    assert!(!root.join("old_metadata.txt").exists());
    assert!(root.join("new_metadata.txt").exists());
    assert_eq!(
      fs::read_to_string(root.join("new_metadata.txt")).unwrap(),
      "content"
    );
  }
);

test_patch_ok!(
  applies_patch_with_multiple_hunks,
  initial_fs: { "file.txt" => "line 1\nline 2\nline 3\nline 4\nline 5\n" },
  diff: r#"
    diff --git a/file.txt b/file.txt
    index abcdef0..abcdef0
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
    -line 1
    -line 2
    +new line 1
    +new line 2
    @@ -4,2 +4,2 @@
    -line 4
    -line 5
    +new line 4
    +new line 5
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      "new line 1\nnew line 2\nline 3\nnew line 4\nnew line 5\n"
    );
  }
);

test_patch_ok!(
  copies_file,
  initial_fs: { "old_file.txt" => "content" },
  diff: r#"
    diff --git a/old_file.txt b/new_file.txt
    copy from old_file.txt
    copy to new_file.txt
  "#,
  assertions: |root| {
    assert!(root.join("old_file.txt").exists());
    assert!(root.join("new_file.txt").exists());
    assert_eq!(
      fs::read_to_string(root.join("new_file.txt")).unwrap(),
      "content"
    );
  }
);

test_apply_ok!(
  handles_empty_lines,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,5 +1,5 @@
     line 1

     line 3
    -line 4
    +new line 4
     line 5
  "#,
  "line 1\n\nline 3\nline 4\nline 5\n",
  "line 1\n\nline 3\nnew line 4\nline 5\n"
);

test_patch_err!(
  fails_on_whitespace_in_context_mismatch,
  initial_fs: { "file.txt" => "    context line\ndeletion line\n" },
  diff: r#"
    diff --git a/file.txt b/file.txt
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
       context line
    -deletion line
    +addition line
  "#
);

test_patch_err!(
  fails_on_whitespace_in_deletion_mismatch,
  initial_fs: { "file.txt" => " context line\n   deletion line\n" },
  diff: r#"
    diff --git a/file.txt b/file.txt
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
     context line
    -  deletion line
    +addition line
  "#
);

test_patch_ok!(
  creates_empty_binary_file,
  initial_fs: {},
  diff: r#"
    diff --git a/image.png b/image.png
    new file mode 100644
    index 0000000..8989898
    Binary files /dev/null and b/image.png differ
  "#,
  assertions: |root| {
    assert!(root.join("image.png").exists());
    assert_eq!(fs::read(root.join("image.png")).unwrap().len(), 0);
  }
);

test_patch_ok!(
  creates_file_in_new_directory,
  initial_fs: {},
  diff: r#"
    diff --git a/new/dir/file.txt b/new/dir/file.txt
    new file mode 100644
    index 0000000..abcdef0
    --- /dev/null
    +++ b/new/dir/file.txt
    @@ -0,0 +1 @@
    +hello world
  "#,
  assertions: |root| {
    assert!(root.join("new/dir/file.txt").exists());
    assert_eq!(
      fs::read_to_string(root.join("new/dir/file.txt")).unwrap(),
      "hello world\n"
    );
    assert!(root.join("new/dir").is_dir());
  }
);

test_apply_ok!(
  no_change_when_patch_has_only_context,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,3 +1,3 @@
     context 1
     context 2
     context 3
  "#,
  "context 1\ncontext 2\ncontext 3\n",
  "context 1\ncontext 2\ncontext 3\n"
);

test_patch_ok!(
  applies_patch_to_empty_file,
  initial_fs: { "empty.txt" => "" },
  diff: r#"
    diff --git a/empty.txt b/empty.txt
    index 0000000..abcdef0 100644
    --- a/empty.txt
    +++ b/empty.txt
    @@ -0,0 +1,2 @@
    +line 1
    +line 2
  "#,
  assertions: |root| {
    assert!(root.join("empty.txt").exists());
    assert_eq!(
      fs::read_to_string(root.join("empty.txt")).unwrap(),
      "line 1\nline 2\n"
    );
  }
);

test_parser_err!(
  fails_on_missing_file_header,
  "-hello\n+world",
  ErrorKind::PatchHasContentButNoFileInfo
);

#[cfg(unix)]
test_patch_ok!(
  changes_file_mode,
  initial_fs: { "file.txt" => "hello\n" },
  diff: r#"
    diff --git a/file.txt b/file.txt
    old mode 100644
    new mode 100755
  "#,
  assertions: |root| {
    use std::os::unix::fs::PermissionsExt;
    assert!(root.join("file.txt").exists());
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      "hello\n"
    );
    let permissions = fs::metadata(root.join("file.txt"))
      .unwrap()
      .permissions();
    assert_eq!(permissions.mode(), 0o100755);
  }
);

#[cfg(not(unix))]
test_patch_ok!(
  does_not_change_file_mode_when_unsupported,
  initial_fs: { "file.txt" => "hello\n" },
  diff: r#"
    diff --git a/file.txt b/file.txt
    old mode 100644
    new mode 100755
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      "hello\n"
    );
  }
);

test_parser_err!(
  fails_on_unexpected_eof,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,1 +1,1 @@
    -hello
  "#,
  ErrorKind::HunkLineCountMismatch
);
