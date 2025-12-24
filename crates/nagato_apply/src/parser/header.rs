use nagato_core::error::Error;

use crate::{lexer::TokenKind, models::Patch};

pub fn parse_header<'a>(
  parser: &mut crate::parser::Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<(), Error> {
  while let Some(Ok(item)) = parser.tokens.peek() {
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
        patch.index_mode = *mode;
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
        patch.new_mode = Some(*mode);
      }
      TokenKind::OldFileMode(mode) => {
        patch.old_mode = Some(*mode);
      }
      TokenKind::DeletedFileMode(mode) => {
        patch.deleted_mode = Some(*mode);
      }
      TokenKind::Similarity(percent) => {
        patch.similarity = Some(*percent);
      }
      TokenKind::Dissimilarity(p) => {
        patch.dissimilarity = Some(*p);
      }
      TokenKind::Binary { old_file, new_file } => {
        patch.old_file = old_file;
        patch.new_file = new_file;
        patch.binary = true;
        parser.tokens.next();
      }
      TokenKind::GitBinaryPatchHeader => {
        parser.tokens.next();
        crate::parser::binary::parse_binary_patch(parser, patch)?;
        return Ok(());
      }
      _ => break,
    }
    parser.tokens.next();
  }
  Ok(())
}
