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
