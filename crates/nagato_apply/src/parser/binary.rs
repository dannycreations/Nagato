use nagato_core::{parse_int, Error};

use crate::{BinaryFragment, BinaryKind, Parser, Patch, TokenKind};

pub fn parse_binary_patch<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
  binary_fragments: &mut Vec<BinaryFragment<'a>>,
) -> Result<(), Error> {
  patch.binary = true;
  // Binary patches are parsed by consuming type-specific headers followed by sequential blocks of encoded binary data.
  while let Some(item) = parser.peek_token()? {
    match item.token {
      TokenKind::BinaryPatchType { kind, size } => {
        parser.tokens.next();
        let kind = if kind == b"literal" {
          BinaryKind::Literal
        } else {
          BinaryKind::Delta
        };
        let (size, _) = parse_int::<u64>(size, 10).unwrap_or((0, &[]));

        // Pre-allocate binary data buffer.
        // Git base85 encodes 5 characters into 4 bytes, with max 52 decoded bytes per line.
        let mut data = Vec::with_capacity((size / 52) as usize + 1);
        while let Some(item) = parser.peek_token()? {
          if let TokenKind::BinaryData(line) = item.token {
            data.push(line);
            parser.tokens.next();
          } else {
            break;
          }
        }
        binary_fragments.push(BinaryFragment { kind, size, data });
      }
      TokenKind::Context(_) => {
        parser.tokens.next();
      }
      _ => break,
    }
  }
  Ok(())
}
