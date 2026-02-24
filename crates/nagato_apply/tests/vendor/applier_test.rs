use std::fs;

test_apply_ok!(
  applies_hunkless,
  r#"
    --- a/file.txt
    +++ b/file.txt
    -hello
    +world
  "#,
  "hello\n",
  "world\n"
);

test_apply_ok!(
  applies_with_offset,
  r#"
    --- a/file.txt
    +++ b/file.txt
    -hello
    +world
  "#,
  "some other content\nhello\n",
  "some other content\nworld\n"
);

test_apply_ok!(
  applies_with_context,
  r#"
    --- a/file.txt
    +++ b/file.txt
     context before
    -hello
    +world
     context after
  "#,
  "some other content\ncontext before\nhello\ncontext after\n",
  "some other content\ncontext before\nworld\ncontext after\n"
);

test_apply_ok!(
  applies_multi_hunkless,
  r#"
    file a/file.txt
     line1
    -line2
    +modified2

     line4
    -line5
    +modified5
  "#,
  "line1\nline2\nline3\nline4\nline5\n",
  "line1\nmodified2\nline3\nline4\nmodified5\n"
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
    let needle1 = "Block A {\n    item {\n        val: 1\n    }\n}";
    let needle2 = "item {\n        val: 3\n        val: 2\n    }";
    assert!(content.contains(needle1));
    assert!(content.contains(needle2));
  }
);

test_apply_ok!(
  applies_multi_hunkless_backward,
  r#"
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
  "context 1\nsame\ncontext 2\nsame\ncontext 3\nsame\n",
  "context 1\nchanged 3\ncontext 2\nchanged 2\ncontext 3\nchanged 1\n"
);

test_patch_ok!(
  applies_hunkless_with_label,
  initial_fs: { "file.txt" => "function a() {\n// content\n}\n\nfunction b() {\n// content\n}\n" },
  diff: r#"
    file a/file.txt
    label function b() {
    -// content
    +// modified content
  "#,
  assertions: |root| {
    let content = fs::read_to_string(root.join("file.txt")).unwrap();
    let needle1 = "function b() {\n// modified content\n}";
    let needle2 = "function a() {\n// content\n}";
    assert!(content.contains(needle1));
    assert!(content.contains(needle2));
  }
);

test_apply_ok!(
  applies_hunkless_with_repeated_label,
  r#"
    file a/file.txt
    label Container {

         Block 1 {
    -        // item 1
    +        // modified 1
         }

    label Container {

         Block 2 {
    -        // item 2
    +        // modified 2
         }
  "#,
  "Container {\n    Block 1 {\n        // item 1\n    }\n    Block 2 {\n        // item 2\n    }\n}\n",
  "Container {\n    Block 1 {\n        // modified 1\n    }\n    Block 2 {\n        // modified 2\n    }\n}\n"
);

test_patch_ok!(
  applies_hunkless_with_multiple_labels_and_empty_lines,
  initial_fs: { "file.txt" => "Header {\n  Body {\n    value: 1\n  }\n\n  // Comment\n}\n\nHeader {\n  Body {\n    value: 1\n  }\n\n  // Comment\n}\n" },
  diff: r#"
    file file.txt
    label Header {

       Body {
    +    new: true
         value: 1
       }

       // Comment

    label Header {

       Body {
    +    new: true
         value: 1
       }

       // Comment
  "#,
  assertions: |root| {
    let content = fs::read_to_string(root.join("file.txt")).unwrap();
    let needle = "Body {\n    new: true\n    value: 1\n  }";
    assert_eq!(content.matches(needle).count(), 2);
  }
);

test_patch_ok!(
  applies_hunkless_with_multiple_labels_and_empty_lines_ordered,
  initial_fs: { "file.txt" => "Header {\n  Body {\n    value: 1\n  }\n\n  // Comment\n}\n\nHeader {\n  Body {\n    value: 1\n  }\n\n  // Comment\n}\n" },
  diff: r#"
    file file.txt
    label Header {

       Body {
    +    new: true
         value: 1
       }

       // Comment
  "#,
  assertions: |root| {
    let content = fs::read_to_string(root.join("file.txt")).unwrap();
    let needle = "Body {\n    new: true\n    value: 1\n  }\n\n  // Comment\n}\n\nHeader {\n  Body {\n    value: 1\n  }";
    assert!(content.contains(needle));
  }
);

test_apply_ok!(
  applies_addition_after_no_newline,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,1 +1,2 @@
     line1
    \ No newline at end of file
    +line2
  "#,
  "line1",
  "line1\nline2\n"
);

test_apply_ok!(
  applies_addition_after_no_newline_remains_no_newline,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,1 +1,2 @@
     line1
    \ No newline at end of file
    +line2
    \ No newline at end of file
  "#,
  "line1",
  "line1\nline2"
);

test_apply_ok!(
  applies_deletion_and_addition_with_no_newline,
  r#"
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
     line1
    -line2
    \ No newline at end of file
    +line3
    \ No newline at end of file
  "#,
  "line1\nline2",
  "line1\nline3"
);

test_apply_ok!(
  applies_hunkless_with_gap_vs_context_empty,
  r#"
    file a/file.ts
    label a(b) {

     				break
     			}
     
    -			if (b) {
    -			a

    label a(b) {

     				break
     			}
     
    -			if (b) {
    -			b
  "#,
  "label a(b) {\n\n				break\n			}\n\n			if (b) {\n			a\n\nlabel a(b) {\n\n				break\n			}\n\n			if (b) {\n			b\n",
  "label a(b) {\n\n				break\n			}\n\n\nlabel a(b) {\n\n				break\n			}\n\n"
);
