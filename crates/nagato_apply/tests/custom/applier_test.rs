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
  applies_ambiguous_hunkless_sequential,
  initial_fs: { "file.txt" => "Block A {\n    item {\n        val: 1\n    }\n}\n\nBlock B {\n    // Some context\n    let x = 1;\n\n    item {\n        val: 2\n    }\n}\n" },
  diff: r#"
    file a/file.txt
     Block B {

         item {
    +        val: 3
             val: 2
  "#,
  assertions: |root| {
    let content = fs::read_to_string(root.join("file.txt")).unwrap();
    // The first block should NOT be modified
    assert!(content.contains("Block A {\n    item {\n        val: 1\n    }\n}"));
    // The second block SHOULD be modified
    assert!(content.contains("item {\n        val: 3\n        val: 2\n    }"));
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

test_patch_ok!(
  applies_hunkless_with_repeated_label,
  initial_fs: { "file.txt" => "Container {\n    Block 1 {\n        // item 1\n    }\n    Block 2 {\n        // item 2\n    }\n}\n" },
  diff: r#"
    file a/file.txt
     Container {

         Block 1 {
    -        // item 1
    +        // modified 1
         }

     Container {

         Block 2 {
    -        // item 2
    +        // modified 2
         }
  "#,
  assertions: |root| {
    let content = fs::read_to_string(root.join("file.txt")).unwrap();
    assert_eq!(
      content,
      "Container {\n    Block 1 {\n        // modified 1\n    }\n    Block 2 {\n        // modified 2\n    }\n}\n"
    );
  }
);

test_patch_ok!(
  applies_with_label_line,
  initial_fs: { "file.txt" => "function a() {\n// content\n}\n\nfunction b() {\n// content\n}\n" },
  diff: r#"
    file a/file.txt
    label function b() {
    -// content
    +// modified content
  "#,
  assertions: |root| {
    let content = fs::read_to_string(root.join("file.txt")).unwrap();
    // Function b should be changed
    assert!(content.contains("function b() {\n// modified content\n"));
    // Function a should be unchanged
    assert!(content.contains("function a() {\n// content\n}"));
  }
);
