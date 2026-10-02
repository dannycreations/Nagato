use nagato_core::{next_path_pair, parse_int, unquote_path, Error};

use crate::{parser::binary::parse_binary_patch, Parser, Patch, TokenKind};

pub fn parse_header<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<(), Error> {
  while let Some(item) = parser.peek_token()? {
    match &item.token {
      TokenKind::FileHeader(path) => {
        let (old, new) = next_path_pair(path, b"")
          .unwrap_or_else(|| (unquote_path(path), unquote_path(path)));
        patch.old_file = old;
        patch.new_file = new;
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
        patch.old_file = unquote_path(file);
      }
      TokenKind::NewFile(file) => {
        patch.new_file = unquote_path(file);
      }
      TokenKind::CopyFrom(from) => {
        patch.copy_from = Some(unquote_path(from));
      }
      TokenKind::CopyTo(to) => {
        patch.copy_to = Some(unquote_path(to));
      }
      TokenKind::RenameFrom(from) => {
        patch.rename_from = Some(unquote_path(from));
      }
      TokenKind::RenameTo(to) => {
        patch.rename_to = Some(unquote_path(to));
      }
      TokenKind::NewFileMode(mode) => {
        patch.new_mode = parse_int::<u32>(mode, 8).map(|(v, _)| v);
      }
      TokenKind::OldFileMode(mode) | TokenKind::DeletedFileMode(mode) => {
        patch.old_mode = parse_int::<u32>(mode, 8).map(|(v, _)| v);
      }
      TokenKind::Similarity(percent) => {
        patch.similarity = Some(*percent);
      }
      TokenKind::Dissimilarity(p) => {
        patch.dissimilarity = Some(*p);
      }
      TokenKind::Binary(path) => {
        // "a/x and b/y" is the standard form; fall back to a bare pair for
        // the lines that carry only one path.
        let (old, new) = next_path_pair(path, b"and ")
          .or_else(|| next_path_pair(path, b""))
          .unwrap_or_else(|| (unquote_path(path), unquote_path(path)));
        patch.old_file = old;
        patch.new_file = new;
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
