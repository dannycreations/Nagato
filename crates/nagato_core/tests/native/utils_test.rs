use nagato_core::{
  get_line, next_path, parse_int, split_diff_paths, strip_diff_prefix,
  unquote_path,
};

test_get_line!(
  utils_get_line_lf,
  input: b"line\nrest",
  expected: Some((&b"line"[..], &b"rest"[..]))
);
test_get_line!(
  utils_get_line_crlf,
  input: b"line\r\nrest",
  expected: Some((&b"line"[..], &b"rest"[..]))
);
test_get_line!(
  utils_get_line_eof,
  input: b"last",
  expected: Some((&b"last"[..], &b""[..]))
);
test_get_line!(
  utils_get_line_empty,
  input: b"",
  expected: None
);

test_parse_int!(
  utils_parse_int_empty,
  type: u32,
  input: b"",
  radix: 10,
  expected: None
);
test_parse_int!(
  utils_parse_int_non_digit,
  type: u32,
  input: b"z123",
  radix: 10,
  expected: None
);
test_parse_int!(
  utils_parse_int_max,
  type: u8,
  input: b"255",
  radix: 10,
  expected: Some((255, &[][..]))
);
test_parse_int!(
  utils_parse_int_overflow,
  type: u8,
  input: b"256",
  radix: 10,
  expected: None
);
test_parse_int!(
  utils_parse_int_hex,
  type: u32,
  input: b"deadbeef",
  radix: 16,
  expected: Some((0xdeadbeef, &[][..]))
);
test_parse_int!(
  utils_parse_int_bin,
  type: u32,
  input: b"1010",
  radix: 2,
  expected: Some((10, &[][..]))
);
test_parse_int!(
  utils_parse_int_base36,
  type: u32,
  input: b"1z",
  radix: 36,
  expected: Some((71, &[][..]))
);
test_parse_int!(
  utils_parse_int_base36_caps,
  type: u32,
  input: b"1Z",
  radix: 36,
  expected: Some((71, &[][..]))
);
test_parse_int!(
  utils_parse_int_invalid_digit_bin,
  type: u32,
  input: b"2",
  radix: 2,
  expected: None
);
test_parse_int!(
  utils_parse_int_invalid_digit_hex,
  type: u32,
  input: b"G",
  radix: 16,
  expected: None
);
test_parse_int!(
  utils_parse_int_valid_digit_base17,
  type: u32,
  input: b"G",
  radix: 17,
  expected: Some((16, &[][..]))
);
test_parse_int!(
  utils_parse_int_radix_limit,
  type: u32,
  input: b"8",
  radix: 8,
  expected: None
);
test_parse_int!(
  utils_parse_int_radix_max,
  type: u32,
  input: b"7",
  radix: 8,
  expected: Some((7, &[][..]))
);
test_parse_int!(
  utils_parse_int_partial_oct,
  type: u32,
  input: b"18",
  radix: 8,
  expected: Some((1, &b"8"[..]))
);
test_parse_int!(
  utils_parse_int_partial,
  type: u32,
  input: b"123abc456",
  radix: 10,
  expected: Some((123, &b"abc456"[..]))
);

#[test]
fn test_next_path_edge_cases() {
  // Verifying that next_path correctly identifies boundaries when escapes are present inside quotes.
  let input = b"\"path with \\\"quote\\\"\" rest";
  let (path, rest) = next_path(input).unwrap();
  assert_eq!(path, b"\"path with \\\"quote\\\"\"");
  assert_eq!(rest, b"rest");

  let input2 = b"\"path with \\\\backslashes\" rest";
  let (path2, rest2) = next_path(input2).unwrap();
  assert_eq!(path2, b"\"path with \\\\backslashes\"");
  assert_eq!(rest2, b"rest");
}

#[test]
fn test_split_diff_paths_with_escaped_spaces() {
  // Testing split_diff_paths with quoted paths containing spaces.
  let line = b" \"a/file name.txt\" \"b/file name.txt\"";
  let (p1, p2) = split_diff_paths(line).unwrap();
  assert_eq!(p1.as_ref(), b"file name.txt");
  assert_eq!(p2.as_ref(), b"file name.txt");
}

test_unquote_path!(
  utils_unquote_unmatched_start,
  input: b"\"no_end_quote",
  expected: b"\"no_end_quote"
);
test_unquote_path!(
  utils_unquote_unmatched_end,
  input: b"no_start_quote\"",
  expected: b"no_start_quote\""
);
test_unquote_path!(
  utils_unquote_prefix_a,
  input: b"\"a/file.txt\"",
  expected: b"file.txt"
);
test_unquote_path!(
  utils_unquote_prefix_b,
  input: b"\"b/file.txt\"",
  expected: b"file.txt"
);
test_unquote_path!(
  utils_unquote_escape_n,
  input: b"\"line\\nfeed\"",
  expected: b"line\nfeed"
);
test_unquote_path!(
  utils_unquote_escape_r,
  input: b"\"carriage\\rreturn\"",
  expected: b"carriage\rreturn"
);
test_unquote_path!(
  utils_unquote_escape_t,
  input: b"\"tab\\tcharacter\"",
  expected: b"tab\tcharacter"
);
test_unquote_path!(
  utils_unquote_escape_backslash,
  input: b"\"backslash\\\\backslash\"",
  expected: b"backslash\\backslash"
);
test_unquote_path!(
  utils_unquote_escape_quote,
  input: b"\"quote\\\"quote\"",
  expected: b"quote\"quote"
);
test_unquote_path!(
  utils_unquote_escape_unknown,
  input: b"\"unknown\\xescape\"",
  expected: b"unknownxescape"
);
test_unquote_path!(
  utils_unquote_octal_1,
  input: b"\"\\1\"",
  expected: b"\x01"
);
test_unquote_path!(
  utils_unquote_octal_2,
  input: b"\"\\12\"",
  expected: b"\x0a"
);
test_unquote_path!(
  utils_unquote_octal_3,
  input: b"\"\\123\"",
  expected: b"S"
);
test_unquote_path!(
  utils_unquote_octal_invalid,
  input: b"\"\\8\"",
  expected: b"8"
);
test_unquote_path!(
  utils_unquote_octal_prefix_a,
  input: b"\"\\141/quoted_path\"",
  expected: b"quoted_path"
);
test_unquote_path!(
  utils_unquote_octal_prefix_b,
  input: b"\"\\142/quoted_path\"",
  expected: b"quoted_path"
);

test_strip_prefix!(
  utils_strip_prefix_a,
  input: b"a/path/to/file",
  expected: b"path/to/file"
);
test_strip_prefix!(
  utils_strip_prefix_b,
  input: b"b/path/to/file",
  expected: b"path/to/file"
);
test_strip_prefix!(
  utils_strip_prefix_none,
  input: b"c/path/to/file",
  expected: b"c/path/to/file"
);
test_strip_prefix!(
  utils_strip_prefix_a_empty,
  input: b"a/",
  expected: b""
);
test_strip_prefix!(
  utils_strip_prefix_b_empty,
  input: b"b/",
  expected: b""
);

test_line_writer_ok!(
  test_line_writer_behavior,
  assertions: |writer, buf| {
    assert!(writer.is_first_line());

    writer.write_bytes(b"").unwrap();
    assert!(
      writer.is_first_line(),
      "Empty write should not change first line state"
    );

    writer.write_bytes(b"data").unwrap();
    assert!(!writer.is_first_line());

    let mut buf2 = Vec::new();
    let mut writer2 = LineWriter::new(&mut buf2);
    writer2.write_newline().unwrap();
    assert!(!writer2.is_first_line());

    // Consecutive ensures
    let mut buf3 = Vec::new();
    {
      let mut w = LineWriter::new(&mut buf3);
      w.ensure_newline().unwrap(); // First line, does nothing
    }
    assert_eq!(buf3, b"");

    {
      let mut w = LineWriter::new(&mut buf3);
      w.write_bytes(b"first").unwrap();
      w.ensure_newline().unwrap(); // Now not first line, should add \n
      w.ensure_newline().unwrap(); // Should add another \n
    }
    assert_eq!(buf3, b"first\n\n");
  }
);
