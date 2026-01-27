use nagato_core::{parse_int, Error, ErrorKind};

use super::binary::parse_binary_patch;
use crate::{BinaryFragment, Parser, Patch, TokenKind};

pub fn parse_header<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
  binary_fragments: &mut Vec<BinaryFragment<'a>>,
) -> Result<(), Error> {
  // Patch headers are processed by iteratively peeking at tokens and updating patch metadata until a non-header token is encountered.
  while let Some(item) = parser.peek_token()? {
    match &item.token {
      TokenKind::FileHeader(paths) => {
        patch.old_file = paths.old_file.clone();
        patch.new_file = paths.new_file.clone();
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
        patch.old_file = file.clone();
      }
      TokenKind::NewFile(file) => {
        patch.new_file = file.clone();
      }
      TokenKind::CopyFrom(from) => {
        patch.copy_from = Some(from.clone());
      }
      TokenKind::CopyTo(to) => {
        patch.copy_to = Some(to.clone());
      }
      TokenKind::RenameFrom(from) => {
        patch.rename_from = Some(from.clone());
      }
      TokenKind::RenameTo(to) => {
        patch.rename_to = Some(to.clone());
      }
      TokenKind::NewFileMode(mode) => {
        patch.new_mode = parse_int::<u32>(mode, 8).map(|(v, _)| v);
      }
      TokenKind::OldFileMode(mode) | TokenKind::DeletedFileMode(mode) => {
        patch.old_mode = parse_int::<u32>(mode, 8).map(|(v, _)| v);
      }
      TokenKind::Similarity(percent) => {
        patch.similarity = parse_percentage(percent).ok();
      }
      TokenKind::Dissimilarity(p) => {
        patch.dissimilarity = parse_percentage(p).ok();
      }
      TokenKind::Binary(paths) => {
        patch.old_file = paths.old_file.clone();
        patch.new_file = paths.new_file.clone();
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
  s.strip_suffix(b"%")
    .and_then(|s| parse_int::<u32>(s, 10))
    .filter(|(_, rest)| rest.is_empty())
    .map(|(num, _)| num)
    .ok_or(ErrorKind::InvalidPercentage)
}
