use std::fs;

test_patch_ok!(
  applies_hunkless,
  initial_fs: { "file.txt" => "hello\n" },
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    -hello
    +world
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      "world\n"
    );
  }
);

test_patch_ok!(
  applies_with_offset,
  initial_fs: { "file.txt" => "some other content\nhello\n" },
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
    -hello
    +world
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      "some other content\nworld\n"
    );
  }
);

test_patch_ok!(
  applies_with_context,
  initial_fs: { "file.txt" => "some other content\ncontext before\nhello\ncontext after\n" },
  diff: r#"
    --- a/file.txt
    +++ b/file.txt
     context before
    -hello
    +world
     context after
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      "some other content\ncontext before\nworld\ncontext after\n"
    );
  }
);

test_patch_ok!(
  applies_file_header,
  initial_fs: { "file.txt" => "hello\n" },
  diff: r#"
    file a/file.txt
    -hello
    +world
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      "world\n"
    );
  }
);

test_patch_ok!(
  applies_multi_hunkless,
  initial_fs: { "file.txt" => "line1\nline2\nline3\nline4\nline5\n" },
  diff: r#"
    file a/file.txt
     line1
    -line2
    +modified2

     line4
    -line5
    +modified5
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      "line1\nmodified2\nline3\nline4\nmodified5\n"
    );
  }
);

test_patch_ok!(
  applies_multi_hunkless_backward,
  initial_fs: { "file.txt" => "context 1\nsame\ncontext 2\nsame\ncontext 3\nsame\n" },
  diff: r#"
    file a/file.txt
     context 3
    -same
    +changed 1

     context 2
    -same
    +changed 2

     context 1
    -same
    +changed 3
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read_to_string(root.join("file.txt")).unwrap(),
      "context 1\nchanged 3\ncontext 2\nchanged 2\ncontext 3\nchanged 1\n"
    );
  }
);
