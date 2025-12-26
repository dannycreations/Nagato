use nagato_core::Error;

use crate::{BinaryFragment, Parser, Patch, TokenKind};

pub fn parse_binary_patch<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<(), Error> {
  patch.binary = true;
  while let Some(Ok(item)) = parser.tokens.peek() {
    match item.token {
      TokenKind::BinaryPatchType { kind, size } => {
        parser.tokens.next();
        // Pre-allocate binary data buffer
        let mut data = Vec::with_capacity((size / 70) as usize + 2);
        while let Some(Ok(item)) = parser.tokens.peek() {
          if let TokenKind::BinaryData(line) = item.token {
            data.push(line);
            parser.tokens.next();
          } else {
            break;
          }
        }
        patch
          .binary_fragments
          .push(BinaryFragment { kind, size, data });
      }
      TokenKind::Context(_) => {
        parser.tokens.next();
      }
      _ => break,
    }
  }
  Ok(())
}
