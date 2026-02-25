use nagato_apply::Parser;

#[test]
fn test_parser_hunk_label_precedence() {
  let input = b"--- a/file.txt\n+++ b/file.txt\nlabel outer\n@@ -1,1 +1,1 @@ inner\n-old\n+new\n";
  let mut parser = Parser::new(input);
  let patch = parser.next().unwrap().unwrap();
  assert_eq!(patch.hunks[0].label, Some(b"inner"[..].as_ref()));
}
