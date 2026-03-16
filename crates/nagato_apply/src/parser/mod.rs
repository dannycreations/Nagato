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
    // Patch initialization involves parsing the header and associated hunks into a default patch structure.
    let mut patch = Patch::default();
    let mut binary_fragments = Vec::new();
    let mut hunks = Vec::new();

    let start_line = self.peek_token()?.map(|i| i.line_num).unwrap_or(0);

    header::parse_header(self, &mut patch, &mut binary_fragments)?;
    hunk::parse_hunks(self, &mut patch, &mut hunks)?;

    patch.binary_fragments = binary_fragments.into_boxed_slice();
    patch.hunks = hunks.into_boxed_slice();

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

impl<'a> Parser<'a> {
  pub fn apply_to_fs(
    fs: &nagato_core::FileSystem,
    input: &'a [u8],
    reverse: bool,
  ) -> Result<(), Error> {
    for patch in Self::new(input) {
      crate::patch_file(fs, patch?, reverse)?;
    }
    Ok(())
  }
}
