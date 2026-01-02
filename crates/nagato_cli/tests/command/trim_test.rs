test_exec_ok!(
  exec_trim_command,
  initial_fs: {},
  diff: r#"
    diff --git a/foo.txt b/foo.txt
    index 1234567..89abcde 100644
    --- a/foo.txt
    +++ b/foo.txt
    @@ -1,3 +1,3 @@
     context
    -old
    +new
     context
  "#,
  args: ["trim"],
  patch_name: "test.patch",
  assert_file: ("test.trim.patch", "file foo.txt\n\n context\n-old\n+new\n context\n")
);

test_exec_ok!(
  exec_trim_command_with_label,
  initial_fs: {},
  diff: r#"
    --- a/bar.txt
    +++ b/bar.txt
    @@ -1,1 +1,1 @@ my_label
    -delete
    +add
  "#,
  args: ["trim"],
  patch_name: "test_label.patch",
  assert_file: ("test_label.trim.patch", "file bar.txt\nlabel my_label\n\n-delete\n+add\n")
);

test_exec_ok!(
  exec_trim_command_split,
  initial_fs: {},
  diff: r#"
    --- a/foo.txt
    +++ b/foo.txt
    @@ -1,1 +1,1 @@
    -foo
    +FOO
    --- a/bar.txt
    +++ b/bar.txt
    @@ -1,1 +1,1 @@
    -bar
    +BAR
  "#,
  args: ["trim", "--split"],
  patch_name: "multi.patch",
  assert_file: (
    "foo.txt.trim.patch",
    "file foo.txt\n\n-foo\n+FOO\n"
  ),
  assert_file2: (
    "bar.txt.trim.patch",
    "file bar.txt\n\n-bar\n+BAR\n"
  )
);
