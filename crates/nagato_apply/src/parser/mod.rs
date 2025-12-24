use std::iter::Peekable;

use nagato_core::error::Error;

use crate::{
  lexer::{Lexer, TokenKind},
  models::Patch,
};

pub mod binary;
pub mod header;
pub mod hunk;

pub struct Parser<'a> {
  pub(crate) tokens: Peekable<Lexer<'a>>,
}

impl<'a> Parser<'a> {
  pub fn new(input: &'a [u8]) -> Self {
    Self {
      tokens: Lexer::new(input).peekable(),
    }
  }

  fn parse_patch(&mut self) -> Result<Patch<'a>, Error> {
    let mut patch = Patch::default();
    header::parse_header(self, &mut patch)?;
    self.skip_empty_context_lines();
    hunk::parse_hunks(self, &mut patch)?;

    if patch.hunks.is_empty() && patch.binary_fragments.is_empty() {
      hunk::parse_headerless_hunk(self, &mut patch)?;
    }

    Ok(patch)
  }

  pub(crate) fn skip_empty_context_lines(&mut self) {
    while self.peek_is(|t| matches!(t, TokenKind::Context(s) if s.is_empty())) {
      self.tokens.next();
    }
  }

  pub(crate) fn peek_is(
    &mut self,
    check: impl Fn(&TokenKind<'a>) -> bool,
  ) -> bool {
    if let Some(Ok(item)) = self.tokens.peek() {
      check(&item.token)
    } else {
      false
    }
  }
}

impl<'a> Iterator for Parser<'a> {
  type Item = Result<Patch<'a>, Error>;

  fn next(&mut self) -> Option<Self::Item> {
    self.skip_empty_context_lines();
    self.tokens.peek()?;

    let patch_result = self.parse_patch();
    match patch_result {
      Ok(patch)
        if patch.old_file.is_empty()
          && patch.new_file.is_empty()
          && patch.hunks.is_empty()
          && patch.binary_fragments.is_empty() =>
      {
        None
      }
      res => Some(res),
    }
  }
}
