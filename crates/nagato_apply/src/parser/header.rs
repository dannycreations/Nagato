use nagato_core::Error;

use super::binary::parse_binary_patch;
use crate::{Parser, Patch, TokenKind};

pub fn parse_header<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<(), Error> {
  while let Some(res) = parser.tokens.peek() {
    let item = match res {
      Ok(i) => i,
      Err(_) => return Err(parser.tokens.next().unwrap().unwrap_err()),
    };
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
        parse_binary_patch(parser, patch)?;
        return Ok(());
      }
      _ => break,
    }
    parser.tokens.next();
  }
  Ok(())
}
