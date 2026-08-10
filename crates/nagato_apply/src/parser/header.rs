use nagato_core::{next_path_pair, parse_int, unquote_path, Error};

use crate::{parser::binary::parse_binary_patch, Parser, Patch, TokenKind};

pub fn parse_header<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<(), Error> {
  // Patch headers are processed by iteratively peeking at tokens and updating patch metadata until a non-header token is encountered.
  while let Some(item) = parser.peek_token()? {
    match &item.token {
      TokenKind::FileHeader(paths) => {
        if let Some((old, new)) = next_path_pair(paths.old_file, b"") {
          patch.old_file = old;
          patch.new_file = new;
        } else {
          patch.old_file = unquote_path(paths.old_file);
          patch.new_file = unquote_path(paths.new_file);
        }
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
      TokenKind::Binary(paths) => {
        if let Some((old_file, new_file)) =
          next_path_pair(paths.old_file, b"and ")
        {
          patch.old_file = old_file;
          patch.new_file = new_file;
        } else {
          // Fallback for cases where it's not a standard pair (e.g. diff --git)
          let paths = next_path_pair(paths.old_file, b"").unwrap_or((
            unquote_path(paths.old_file),
            unquote_path(paths.new_file),
          ));
          patch.old_file = paths.0;
          patch.new_file = paths.1;
        }
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
