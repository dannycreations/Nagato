use nagato_apply::{Hunk, Line, LineKind, Matcher};

#[test]
fn test_match_hunk_with_no_matchable_lines() {
  // A hunk with only additions has no "lines to match" (no context or deletions).
  let hunk = Hunk {
    lines: vec![Line {
      kind: LineKind::Addition,
      text: b"new line",
    }],
    ..Default::default()
  };

  let buffer = b"existing content";
  let matcher = Matcher;

  // This should not panic and should match at position 0.
  let res = matcher.find_match(buffer, &hunk, None);
  assert!(res.is_ok());
  let (pos, remaining) = res.unwrap();
  assert_eq!(pos, 0);
  assert_eq!(remaining, buffer);
}

#[test]
fn test_match_hunk_with_only_empty_context() {
  // A hunk with context lines that are empty (e.g. just a newline in the file).
  let hunk = Hunk {
    lines: vec![
      Line {
        kind: LineKind::Context,
        text: b"",
      },
      Line {
        kind: LineKind::Addition,
        text: b"new line",
      },
    ],
    ..Default::default()
  };

  let buffer = b"\nexisting content";
  let matcher = Matcher;

  // This should match the empty line.
  let res = matcher.find_match(buffer, &hunk, None);
  assert!(res.is_ok());
  let (pos, remaining) = res.unwrap();
  assert_eq!(pos, 0);
  assert_eq!(remaining, b"existing content");
}
