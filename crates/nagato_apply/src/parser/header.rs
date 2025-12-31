use nagato_core::{parse_int, Error, ErrorKind, ParseErrorKind};

use super::binary::parse_binary_patch;
use crate::{BinaryFragment, Parser, Patch, TokenKind};

pub fn parse_header<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
  binary_fragments: &mut Vec<BinaryFragment<'a>>,
) -> Result<(), Error> {
  while let Some(item) = parser.peek_token()? {
    match &item.token {
      TokenKind::FileHeader { old_file, new_file } => {
        patch.old_file = old_file;
        patch.new_file = new_file;
      }
      TokenKind::Index {
        old_hash,
        new_hash,
        mode,
      } => {
        patch.old_hash = Some(old_hash);
        patch.new_hash = Some(new_hash);
        patch.new_mode = patch.new_mode.or_else(|| {
          mode.and_then(|m| parse_int::<u32>(m, 8).map(|(v, _)| v))
        });
      }
      TokenKind::OldFile(file) => {
        patch.old_file = file;
      }
      TokenKind::NewFile(file) => {
        patch.new_file = file;
      }
      TokenKind::CopyFrom(from) => {
        patch.copy_from = Some(from);
      }
      TokenKind::CopyTo(to) => {
        patch.copy_to = Some(to);
      }
      TokenKind::RenameFrom(from) => {
        patch.rename_from = Some(from);
      }
      TokenKind::RenameTo(to) => {
        patch.rename_to = Some(to);
      }
      TokenKind::NewFileMode(mode) => {
        patch.new_mode = parse_int::<u32>(mode, 8).map(|(v, _)| v);
      }
      TokenKind::OldFileMode(mode) => {
        patch.old_mode = parse_int::<u32>(mode, 8).map(|(v, _)| v);
      }
      TokenKind::DeletedFileMode(mode) => {
        patch.old_mode = parse_int::<u32>(mode, 8).map(|(v, _)| v);
      }
      TokenKind::Similarity(percent) => {
        patch.similarity = parse_percentage(percent).ok();
      }
      TokenKind::Dissimilarity(p) => {
        patch.dissimilarity = parse_percentage(p).ok();
      }
      TokenKind::Binary { old_file, new_file } => {
        patch.old_file = old_file;
        patch.new_file = new_file;
        patch.binary = true;
        parser.tokens.next();
      }
      TokenKind::GitBinaryPatchHeader => {
        parser.tokens.next();
        parse_binary_patch(parser, patch, binary_fragments)?;
        return Ok(());
      }
      _ => break,
    }
    parser.tokens.next();
  }
  Ok(())
}

fn parse_percentage(s: &[u8]) -> Result<u32, ErrorKind> {
  let s = s
    .strip_suffix(b"%")
    .ok_or(ParseErrorKind::InvalidPercentage)?;
  let (num, rest) =
    parse_int::<u32>(s, 10).ok_or(ParseErrorKind::InvalidPercentage)?;
  if !rest.is_empty() {
    return Err(ParseErrorKind::InvalidPercentage.into());
  }
  Ok(num)
}
