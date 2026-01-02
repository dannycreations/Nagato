use std::fs;

test_patch_ok!(
  reverses_patch,
  reverse: true,
  initial_fs: { "file.txt" => " context 1\nnew line\n context 2\n" },
  diff: r#"
    diff --git a/file.txt b/file.txt
    index 0000000..0000000
    --- a/file.txt
    +++ b/file.txt
    @@ -1,3 +1,3 @@
      context 1
    -old line
    +new line
      context 2
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      " context 1\nold line\n context 2\n"
    );
  }
);

test_patch_ok!(
  reverses_file_creation,
  reverse: true,
  initial_fs: { "new_file.txt" => "line 1\nline 2\n" },
  diff: r#"
    diff --git a/new_file.txt b/new_file.txt
    new file mode 100644
    index 0000000..0000000
    --- /dev/null
    +++ b/new_file.txt
    @@ -0,0 +1,2 @@
    +line 1
    +line 2
  "#,
  assertions: |root| {
    assert!(!root.join("new_file.txt").exists());
  }
);

test_patch_ok!(
  reverses_file_deletion,
  reverse: true,
  initial_fs: {},
  diff: r#"
    diff --git a/file_to_delete.txt b/file_to_delete.txt
    index 0000000..0000000
    --- a/file_to_delete.txt
    +++ /dev/null
    @@ -1,2 +0,0 @@
    -line 1
    -line 2
  "#,
  assertions: |root| {
    assert!(root.join("file_to_delete.txt").exists());
    assert_eq!(
      fs::read_to_string(root.join("file_to_delete.txt")).unwrap(),
      "line 1\nline 2\n"
    );
  }
);
