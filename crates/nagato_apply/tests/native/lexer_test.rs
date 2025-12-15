use nagato_apply::TokenKind;

test_lexer_ok!(
  lexes_new_mode,
  "new mode 100644",
  TokenKind::NewFileMode(0o100644)
);
test_lexer_ok!(
  lexes_new_file_mode,
  "new file mode 100644",
  TokenKind::NewFileMode(0o100644)
);
test_lexer_ok!(
  lexes_old_mode,
  "old mode 100644",
  TokenKind::OldFileMode(0o100644)
);
test_lexer_ok!(
  lexes_old_file_mode,
  "old file mode 100644",
  TokenKind::OldFileMode(0o100644)
);
test_lexer_ok!(
  lexes_deleted_mode,
  "deleted mode 100644",
  TokenKind::DeletedFileMode(0o100644)
);
test_lexer_ok!(
  lexes_deleted_file_mode,
  "deleted file mode 100644",
  TokenKind::DeletedFileMode(0o100644)
);

test_lexer_ok!(
  lexes_rename_file,
  "rename from old.txt\nrename to new.txt",
  TokenKind::RenameFrom(b"old.txt"),
  TokenKind::RenameTo(b"new.txt")
);

test_lexer_ok!(
  lexes_copy_file,
  "copy from old.txt\ncopy to new.txt",
  TokenKind::CopyFrom(b"old.txt"),
  TokenKind::CopyTo(b"new.txt")
);

test_lexer_ok!(
  lexes_binary_files_differ,
  "Binary files a/old.bin and b/new.bin differ",
  TokenKind::Binary {
    old_file: b"a/old.bin",
    new_file: b"b/new.bin",
  }
);

test_lexer_ok!(
  lexes_simple_diff,
  r#"
    diff --git a/file.txt b/file.txt
    index 1234567..abcdefg 100644
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
    -hello world
    +Hello, world!
     context
  "#,
  TokenKind::FileHeader {
    old_file: b"file.txt",
    new_file: b"file.txt",
  },
  TokenKind::Index {
    old_hash: b"1234567",
    new_hash: b"abcdefg",
    mode: Some(0o100644),
  },
  TokenKind::OldFile(b"file.txt"),
  TokenKind::NewFile(b"file.txt"),
  TokenKind::HunkHeader {
    old_line: 1,
    old_span: 2,
    new_line: 1,
    new_span: 2,
  },
  TokenKind::Deletion(b"hello world"),
  TokenKind::Addition(b"Hello, world!"),
  TokenKind::Context(b"context")
);

test_lexer_ok!(
  lexes_no_newline_at_end_of_file,
  r#"
    diff --git a/file.txt b/file.txt
    index 1234567..abcdefg 100644
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
    -hello
    -world
    \ No newline at end of file
    +hello
    +world
    \ No newline at end of file
  "#,
  TokenKind::FileHeader {
    old_file: b"file.txt",
    new_file: b"file.txt"
  },
  TokenKind::Index {
    old_hash: b"1234567",
    new_hash: b"abcdefg",
    mode: Some(0o100644)
  },
  TokenKind::OldFile(b"file.txt"),
  TokenKind::NewFile(b"file.txt"),
  TokenKind::HunkHeader {
    old_line: 1,
    old_span: 2,
    new_line: 1,
    new_span: 2
  },
  TokenKind::Deletion(b"hello"),
  TokenKind::Deletion(b"world"),
  TokenKind::NoNewline,
  TokenKind::Addition(b"hello"),
  TokenKind::Addition(b"world"),
  TokenKind::NoNewline
);

test_lexer_ok!(
  lexes_hunk_header_with_zero_span,
  "@@ -0,0 +1,3 @@",
  TokenKind::HunkHeader {
    old_line: 0,
    old_span: 0,
    new_line: 1,
    new_span: 3,
  }
);

test_lexer_ok!(
  lexes_malformed_git_prefix,
  "diff --git file.txt b/file.txt",
  TokenKind::FileHeader {
    old_file: b"file.txt",
    new_file: b"file.txt",
  }
);
