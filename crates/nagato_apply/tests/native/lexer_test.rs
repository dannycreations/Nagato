use nagato_apply::Token;

test_lexer_ok!(
  lexes_new_mode,
  "new mode 100644",
  Token::NewFileMode(0o100644)
);
test_lexer_ok!(
  lexes_new_file_mode,
  "new file mode 100644",
  Token::NewFileMode(0o100644)
);
test_lexer_ok!(
  lexes_old_mode,
  "old mode 100644",
  Token::OldFileMode(0o100644)
);
test_lexer_ok!(
  lexes_old_file_mode,
  "old file mode 100644",
  Token::OldFileMode(0o100644)
);
test_lexer_ok!(
  lexes_deleted_mode,
  "deleted mode 100644",
  Token::DeletedFileMode(0o100644)
);
test_lexer_ok!(
  lexes_deleted_file_mode,
  "deleted file mode 100644",
  Token::DeletedFileMode(0o100644)
);

test_lexer_ok!(
  lexes_rename_file,
  "rename from old.txt\nrename to new.txt",
  Token::RenameFrom(b"old.txt"),
  Token::RenameTo(b"new.txt")
);

test_lexer_ok!(
  lexes_copy_file,
  "copy from old.txt\ncopy to new.txt",
  Token::CopyFrom(b"old.txt"),
  Token::CopyTo(b"new.txt")
);

test_lexer_ok!(
  lexes_binary_files_differ,
  "Binary files a/old.bin and b/new.bin differ",
  Token::Binary {
    old_file: b"a/old.bin",
    new_file: b"b/new.bin",
  }
);

test_lexer_ok!(
  lexes_simple_diff,
  r#"diff --git a/file.txt b/file.txt
index 1234567..abcdefg 100644
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
-hello world
+Hello, world!
 context
"#,
  Token::FileHeader {
    old_file: b"file.txt",
    new_file: b"file.txt",
  },
  Token::Index {
    old_hash: "1234567",
    new_hash: "abcdefg",
    mode: Some(0o100644),
  },
  Token::OldFile(b"file.txt"),
  Token::NewFile(b"file.txt"),
  Token::HunkHeader {
    old_line: 1,
    old_span: 2,
    new_line: 1,
    new_span: 2,
  },
  Token::Deletion(b"hello world"),
  Token::Addition(b"Hello, world!"),
  Token::Context(b"context")
);

test_lexer_ok!(
  lexes_no_newline_at_end_of_file,
  r#"diff --git a/file.txt b/file.txt
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
  Token::FileHeader {
    old_file: b"file.txt",
    new_file: b"file.txt"
  },
  Token::Index {
    old_hash: "1234567",
    new_hash: "abcdefg",
    mode: Some(0o100644)
  },
  Token::OldFile(b"file.txt"),
  Token::NewFile(b"file.txt"),
  Token::HunkHeader {
    old_line: 1,
    old_span: 2,
    new_line: 1,
    new_span: 2
  },
  Token::Deletion(b"hello"),
  Token::Deletion(b"world"),
  Token::NoNewline,
  Token::Addition(b"hello"),
  Token::Addition(b"world"),
  Token::NoNewline
);

test_lexer_ok!(
  lexes_hunk_header_with_zero_span,
  "@@ -0,0 +1,3 @@",
  Token::HunkHeader {
    old_line: 0,
    old_span: 0,
    new_line: 1,
    new_span: 3,
  }
);

test_lexer_err!(
  fails_on_malformed_git_prefix,
  "diff --git file.txt b/file.txt"
);
