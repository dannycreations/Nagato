use std::fs::{read_to_string, write};

use assert_cmd::Command;
use nagato_core::create_test_fs;
use predicates::{prelude::predicate::str::is_empty, str::contains};

test_exec_ok!(
  cli_exec_directory_argument,
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
  cli_exec_reverse_argument,
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
  cli_exec_check_argument,
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

test_cli_ok!(
  cli_args_version_flag,
  args: ["--version"],
  stdout_contains: env!("CARGO_PKG_VERSION"),
);

test_cli_fail!(
  cli_args_error_non_existent_patch,
  args: ["non_existent.patch"],
  stderr: "Can't open patch"
);

test_cli_fail!(
  cli_args_error_invalid_stdin,
  args: [],
  stdin: "invalid\n",
  stderr: "<stdin>"
);
