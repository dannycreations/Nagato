test_lexer_ok!(
  lexer_vendor_lexes_trimmed_file_and_labels,
  input: "file path/to/file.txt\nlabel my_custom_label",
  expected: [
    TokenKind::FileHeader(BinaryPaths {
      old_file: b"path/to/file.txt",
      new_file: b"path/to/file.txt",
    }),
    TokenKind::Label(b"my_custom_label")
  ]
);
