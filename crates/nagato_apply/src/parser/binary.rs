use nagato_core::{parse_int, Error};

use crate::{BinaryFragment, BinaryKind, Parser, Patch, TokenKind};

pub fn parse_binary_patch<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
  binary_fragments: &mut Vec<BinaryFragment<'a>>,
) -> Result<(), Error> {
  patch.binary = true;
  while let Some(res) = parser.tokens.peek() {
    let item = match res {
      Ok(i) => i,
      Err(_) => return Err(parser.tokens.next().transpose().unwrap_err()),
    };

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
        while let Some(res) = parser.tokens.peek() {
          let item = match res {
            Ok(i) => i,
            Err(_) => {
              return Err(parser.tokens.next().transpose().unwrap_err())
            }
          };

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
