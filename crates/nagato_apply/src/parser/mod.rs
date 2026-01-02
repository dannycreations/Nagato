use std::iter::Peekable;

use nagato_core::{Error, ErrorKind};

use crate::{Lexer, Patch, TokenKind};

mod binary;
mod header;
mod hunk;

pub struct Parser<'a> {
  pub tokens: Peekable<Lexer<'a>>,
  pub label: Option<&'a [u8]>,
}

impl<'a> Parser<'a> {
  pub fn new(input: &'a [u8]) -> Self {
    Self {
      tokens: Lexer::new(input).peekable(),
      label: None,
    }
  }

  fn parse_patch(&mut self) -> Result<Patch<'a>, Error> {
    // Ensure label state doesn't leak between patches.
    self.label = None;
    let mut patch = Patch::default();
    let mut hunks = Vec::new();
    let mut binary_fragments = Vec::new();

    let start_line = self
      .tokens
      .peek()
      .and_then(|r| r.as_ref().ok())
      .map(|i| i.line_num)
      .unwrap_or(0);

    header::parse_header(self, &mut patch, &mut binary_fragments)?;
    hunk::parse_hunks(self, &mut patch, &mut hunks)?;

    if !hunks.is_empty()
      && patch.old_file.is_empty()
      && patch.new_file.is_empty()
    {
      return Err(Error::with_line(
        ErrorKind::PatchHasContentButNoFileInfo,
        start_line,
      ));
    }

    patch.hunks = hunks.into_boxed_slice();
    patch.binary_fragments = binary_fragments.into_boxed_slice();

    if patch.hunks.is_empty()
      && patch.binary_fragments.is_empty()
      && patch.old_file.is_empty()
      && patch.new_file.is_empty()
    {
      // If we parsed nothing, it's not an error yet, the iterator handles completion.
      return Ok(patch);
    }

    Ok(patch)
  }

  pub fn skip_empty_context_lines(&mut self) -> Result<(), Error> {
    while self
      .peek_is(|t| matches!(t, TokenKind::Context(s) if s.is_empty()))?
    {
      self.tokens.next();
    }
    Ok(())
  }

  pub fn peek_is(
    &mut self,
    check: impl Fn(&TokenKind<'a>) -> bool,
  ) -> Result<bool, Error> {
    match self.tokens.peek() {
      Some(Ok(item)) => Ok(check(&item.token)),
      Some(Err(_)) => Err(self.tokens.next().transpose().unwrap_err()),
      None => Ok(false),
    }
  }

  /// Helper to peek at the next token, handling errors.
  pub fn peek_token(&mut self) -> Result<Option<&crate::LexerItem<'a>>, Error> {
    if let Some(Err(_)) = self.tokens.peek() {
      return Err(self.tokens.next().transpose().unwrap_err());
    }
    match self.tokens.peek() {
      Some(Ok(item)) => Ok(Some(item)),
      _ => Ok(None),
    }
  }
}

impl<'a> Iterator for Parser<'a> {
  type Item = Result<Patch<'a>, Error>;

  fn next(&mut self) -> Option<Self::Item> {
    self.skip_empty_context_lines().ok()?;
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
