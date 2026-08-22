use std::iter::Peekable;

pub mod binary;
pub(crate) mod header;
pub(crate) mod hunk;

use nagato_core::{Error, ErrorKind};

use crate::{Hunk, Lexer, LexerItem, Patch, TokenKind};

pub struct Parser<'a> {
  pub(crate) tokens: Peekable<Lexer<'a>>,
  pub(crate) label: Option<&'a [u8]>,
}

impl<'a> Parser<'a> {
  pub fn new(input: &'a [u8]) -> Self {
    Self {
      tokens: Lexer::new(input).peekable(),
      label: None,
    }
  }

  pub fn next_hunk(
    &mut self,
    patch: &mut Patch<'a>,
  ) -> Result<Option<Hunk<'a>>, Error> {
    hunk::next_hunk(self, patch)
  }

  pub(crate) fn parse_patch_header(
    &mut self,
  ) -> Result<Option<Patch<'a>>, Error> {
    self.label = None;
    self.skip_empty_context_lines()?;

    if self.tokens.peek().is_none() {
      return Ok(None);
    }

    let mut patch = Patch::default();
    header::parse_header(self, &mut patch)?;

    Ok(Some(patch))
  }

  fn parse_patch(&mut self) -> Result<Patch<'a>, Error> {
    // Ensure label state doesn't leak between patches.
    self.label = None;
    let mut patch = Patch::default();

    let start_line = self.peek_token()?.map(|i| i.line_num).unwrap_or(0);

    header::parse_header(self, &mut patch)?;
    while let Some(hunk) = hunk::next_hunk(self, &mut patch)? {
      patch.hunks.push(hunk);
    }

    if !patch.hunks.is_empty()
      && patch.old_file.is_empty()
      && patch.new_file.is_empty()
    {
      return Err(Error::with_line(
        ErrorKind::PatchHasContentButNoFileInfo,
        start_line,
      ));
    }

    Ok(patch)
  }

  pub fn skip_empty_context_lines(&mut self) -> Result<(), Error> {
    while self.peek_is(TokenKind::is_padding)? {
      self.tokens.next();
    }
    Ok(())
  }

  pub fn peek_is(
    &mut self,
    check: impl Fn(&TokenKind<'a>) -> bool,
  ) -> Result<bool, Error> {
    Ok(self.peek_token()?.is_some_and(|i| check(&i.token)))
  }

  pub fn peek_token(&mut self) -> Result<Option<&LexerItem<'a>>, Error> {
    // Lexer errors are pulled off the stream as soon as a peek sees them.
    if self.tokens.peek().is_some_and(|r| r.is_err()) {
      return Err(self.tokens.next().unwrap().unwrap_err());
    }
    Ok(self.tokens.peek().and_then(|r| r.as_ref().ok()))
  }
}

impl<'a> Iterator for Parser<'a> {
  type Item = Result<Patch<'a>, Error>;

  fn next(&mut self) -> Option<Self::Item> {
    if let Err(e) = self.skip_empty_context_lines() {
      return Some(Err(e));
    }

    self.tokens.peek()?;

    let res = self.parse_patch();

    let Ok(ref patch) = res else {
      return Some(res);
    };

    if !patch.has_content_changes() && patch.is_empty() {
      return None;
    }

    Some(res)
  }
}
