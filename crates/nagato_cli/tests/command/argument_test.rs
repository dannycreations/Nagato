test_exec_ok!(
  exec_directory_argument,
  initial_fs: { "project/file.txt" => "hello\n" },
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    -hello
    +world
  "#,
  args: ["--directory", "project"],
  assert_file: ("project/file.txt", "world\n")
);

test_exec_ok!(
  exec_reverse_argument,
  initial_fs: { "file.txt" => "world\n" },
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    -hello
    +world
  "#,
  args: ["--reverse"],
  assert_file: ("file.txt", "hello\n")
);

test_exec_ok!(
  exec_check_argument,
  initial_fs: { "file.txt" => "hello\n" },
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    -hello
    +world
  "#,
  args: ["--check"],
  assert_file: ("file.txt", "hello\n")
);

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
