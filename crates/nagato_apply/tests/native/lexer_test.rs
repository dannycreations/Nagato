use nagato_apply::{BinaryPaths, TokenKind};

test_lexer_ok!(
  lexer_modes,
  input: "new mode 100644\nnew file mode 100644\nold mode 100644\nold file mode 100644\ndeleted mode 100644\ndeleted file mode 100644",
  expected: [
    TokenKind::NewFileMode(b"100644"),
    TokenKind::NewFileMode(b"100644"),
    TokenKind::OldFileMode(b"100644"),
    TokenKind::OldFileMode(b"100644"),
    TokenKind::DeletedFileMode(b"100644"),
    TokenKind::DeletedFileMode(b"100644")
  ]
);

test_lexer_ok!(
  lexer_rename_and_copy,
  input: "rename from old.txt\nrename to new.txt\ncopy from old.txt\ncopy to new.txt",
  expected: [
    TokenKind::RenameFrom(b"old.txt".into()),
    TokenKind::RenameTo(b"new.txt".into()),
    TokenKind::CopyFrom(b"old.txt".into()),
    TokenKind::CopyTo(b"new.txt".into())
  ]
);

test_lexer_ok!(
  lexer_binary_files_differ,
  input: "Binary files a/old.bin and b/new.bin differ\nBinary files \"a/salt and pepper.png\" and \"b/salt and pepper.png\" differ",
  expected: [
    TokenKind::Binary(BinaryPaths {
      old_file: b"old.bin".into(),
      new_file: b"new.bin".into(),
    }),
    TokenKind::Binary(BinaryPaths {
      old_file: b"salt and pepper.png".into(),
      new_file: b"salt and pepper.png".into(),
    })
  ]
);

test_lexer_ok!(
  lexer_simple_diff,
  input: r#"
    diff --git a/file.txt b/file.txt
    index 1234567..abcdefg 100644
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
    -hello world
    +Hello, world!
     context
  "#,
  expected: [
    TokenKind::FileHeader(BinaryPaths {
      old_file: b"file.txt".into(),
      new_file: b"file.txt".into(),
    }),
    TokenKind::Index {
      old_hash: b"1234567",
      new_hash: b"abcdefg",
      mode: Some(b"100644"),
    },
    TokenKind::OldFile(b"file.txt".into()),
    TokenKind::NewFile(b"file.txt".into()),
    TokenKind::HunkHeader {
      old_range: b"1,2",
      new_range: b"1,2",
      label: None,
    },
    TokenKind::Deletion(b"hello world"),
    TokenKind::Addition(b"Hello, world!"),
    TokenKind::Context(b"context")
  ]
);

test_lexer_ok!(
  lexer_no_newline_markers,
  input: r#"
    \ No newline at end of file
  "#,
  expected: [TokenKind::NoNewline]
);

test_lexer_ok!(
  lexer_hunk_headers,
  input: "@@ -0,0 +1,3 @@\n@@ -1,1 +1,1 @@ function_name",
  expected: [
    TokenKind::HunkHeader {
      old_range: b"0,0",
      new_range: b"1,3",
      label: None,
    },
    TokenKind::HunkHeader {
      old_range: b"1,1",
      new_range: b"1,1",
      label: Some(b"function_name"),
    }
  ]
);

test_lexer_ok!(
  test_lexer_binary_mode_transitions,
  input: "GIT binary patch\nliteral 5\ndata\n\ndiff --git a/file b/file",
  expected: [
    TokenKind::GitBinaryPatchHeader,
    TokenKind::BinaryPatchType {
      kind: b"literal",
      size: b"5"
    },
    TokenKind::BinaryData(b"data"),
    TokenKind::Gap,
    TokenKind::FileHeader(nagato_apply::BinaryPaths {
      old_file: b"file".into(),
      new_file: b"file".into()
    })
  ]
);

test_lexer_binary_data_ok!(
  lexer_binary_tokenization_literal_prefix_data,
  input: b"literal_not_a_header\ndelta_not_a_header",
  expected: [b"literal_not_a_header", b"delta_not_a_header"]
);
