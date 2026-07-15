use nagato_apply::{BinaryPaths, Lexer, LexerMode, TokenKind};
use nagato_core::{next_path_pair, split_diff_paths, unquote_path, ErrorKind};

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
    TokenKind::RenameFrom(b"old.txt"),
    TokenKind::RenameTo(b"new.txt"),
    TokenKind::CopyFrom(b"old.txt"),
    TokenKind::CopyTo(b"new.txt")
  ]
);

test_lexer_ok!(
  lexer_binary_files_differ,
  input: "Binary files a/old.bin and b/new.bin differ\nBinary files \"a/salt and pepper.png\" and \"b/salt and pepper.png\" differ",
  expected: [
    TokenKind::Binary(BinaryPaths {
      old_file: b"old.bin",
      new_file: b"new.bin",
    }),
    TokenKind::Binary(BinaryPaths {
      old_file: b"salt and pepper.png",
      new_file: b"salt and pepper.png",
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
      old_file: b"file.txt",
      new_file: b"file.txt",
    }),
    TokenKind::Index {
      old_hash: b"1234567",
      new_hash: b"abcdefg",
      mode: Some(b"100644"),
    },
    TokenKind::OldFile(b"file.txt"),
    TokenKind::NewFile(b"file.txt"),
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
    TokenKind::FileHeader(BinaryPaths {
      old_file: b"file",
      new_file: b"file"
    })
  ]
);

test_lexer_binary_data_ok!(
  lexer_binary_tokenization_literal_prefix_data,
  input: b"literal_not_a_header\ndelta_not_a_header",
  expected: [b"literal_not_a_header", b"delta_not_a_header"]
);

#[test]
fn test_lexer_errors() {
  // Test unexpected line starting with random character
  let mut lexer = Lexer::new(b"xyz\n");
  assert_eq!(
    lexer.next().unwrap().unwrap_err().kind,
    ErrorKind::UnexpectedLine
  );

  // Test index with invalid format (missing ..)
  let mut lexer = Lexer::new(b"index abcdef0\n");
  assert_eq!(
    lexer.next().unwrap().unwrap_err().kind,
    ErrorKind::InvalidIndexHeader
  );

  // Test similarity index with invalid format (missing %)
  let mut lexer = Lexer::new(b"similarity index 100\n");
  assert_eq!(
    lexer.next().unwrap().unwrap_err().kind,
    ErrorKind::InvalidPercentage
  );

  // Test similarity index with non-integer percentage
  let mut lexer = Lexer::new(b"similarity index abc%\n");
  assert_eq!(
    lexer.next().unwrap().unwrap_err().kind,
    ErrorKind::InvalidPercentage
  );

  // Test dissimilarity index with invalid format (missing %)
  let mut lexer = Lexer::new(b"dissimilarity index abc%\n");
  assert_eq!(
    lexer.next().unwrap().unwrap_err().kind,
    ErrorKind::InvalidPercentage
  );

  // Test mode with invalid format (missing "mode" or "file mode")
  let mut lexer = Lexer::new(b"new other\n");
  assert_eq!(
    lexer.next().unwrap().unwrap_err().kind,
    ErrorKind::InvalidFileMode
  );

  // Test @@ header without space
  let mut lexer = Lexer::new(b"@@-1,1 +1,1@@\n");
  assert_eq!(
    lexer.next().unwrap().unwrap_err().kind,
    ErrorKind::UnexpectedLine
  );

  // Test diff header unexpected
  let mut lexer = Lexer::new(b"diff --other a b\n");
  assert_eq!(
    lexer.next().unwrap().unwrap_err().kind,
    ErrorKind::UnexpectedLine
  );

  // Test git binary patch mismatch
  let mut lexer = Lexer::new(b"GIT binary patch header\n");
  assert_eq!(
    lexer.next().unwrap().unwrap_err().kind,
    ErrorKind::UnexpectedLine
  );
}
