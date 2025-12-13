use std::fs;

test_patch_ok!(
    applies_without_hunk_header,
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
    applies_short_diff_header,
    initial_fs: { "file.txt" => "hello\n" },
    diff: r#"
        a/file.txt b/file.txt
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
    applies_even_shorter_diff_header,
    initial_fs: { "file.txt" => "hello\n" },
    diff: r#"
        a/file.txt
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
