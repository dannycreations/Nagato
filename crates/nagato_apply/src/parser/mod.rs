use std::iter::Peekable;

pub mod binary;
pub(crate) mod header;
pub(crate) mod hunk;

use nagato_core::{Error, ErrorKind};

use crate::{Lexer, Patch, TokenKind};

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

  pub fn next_hunk(
    &mut self,
    patch: &mut Patch<'a>,
  ) -> Result<Option<crate::Hunk<'a>>, Error> {
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
    let mut binary_fragments = Vec::new();
    header::parse_header(self, &mut patch, &mut binary_fragments)?;
    patch.binary_fragments = binary_fragments;

    Ok(Some(patch))
  }

  fn parse_patch(&mut self) -> Result<Patch<'a>, Error> {
    // Ensure label state doesn't leak between patches.
    self.label = None;
    // Patch initialization involves parsing the header and associated hunks into a default patch structure.
    let mut patch = Patch::default();
    let mut binary_fragments = Vec::with_capacity(2);
    let mut hunks = Vec::with_capacity(4);

    let start_line = self.peek_token()?.map(|i| i.line_num).unwrap_or(0);

    header::parse_header(self, &mut patch, &mut binary_fragments)?;
    hunk::parse_hunks(self, &mut patch, &mut hunks)?;

    patch.binary_fragments = binary_fragments;
    patch.hunks = hunks;

    // Patch validity is checked by ensuring that any content changes are associated with at least one valid file path.
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
    while self.peek_is(|t| {
      matches!(t, TokenKind::Gap)
        || matches!(t, TokenKind::Context(s) if s.is_empty())
    })? {
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

  pub fn peek_token(&mut self) -> Result<Option<&crate::LexerItem<'a>>, Error> {
    // Token peeking logic identifies lexer errors by inspecting the next available item without consuming it from the stream.
    if self.tokens.peek().is_some_and(|r| r.is_err()) {
      return Err(self.tokens.next().unwrap().unwrap_err());
    }
    Ok(self.tokens.peek().and_then(|r| r.as_ref().ok()))
  }
}

impl<'a> Iterator for Parser<'a> {
  type Item = Result<Patch<'a>, Error>;

  fn next(&mut self) -> Option<Self::Item> {
    // Patch iteration proceeds by skipping leading whitespace and attempting to parse the next patch until the end of the token stream is reached or an error occurs.
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
