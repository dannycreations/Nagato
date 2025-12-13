use std::iter::Peekable;

use nagato_core::error::{Error, ParseError};

use crate::{Hunk, Lexer, Line, Patch, Token};

pub struct Parser<'a> {
  tokens: Peekable<Lexer<'a>>,
}

impl<'a> Parser<'a> {
  pub fn new(input: &'a [u8]) -> Self {
    Self {
      tokens: Lexer::new(input).peekable(),
    }
  }

  fn parse_header(&mut self, patch: &mut Patch<'a>) -> Result<(), Error> {
    // This loop processes all header tokens at the beginning of a patch.
    // It's more efficient than the previous implementation because it avoids
    // the `consumed` flag and simplifies the control flow.
    loop {
      match self.tokens.peek() {
        Some(Ok(token)) => match token {
          Token::FileHeader { old_file, new_file } => {
            patch.old_file = old_file;
            patch.new_file = new_file;
          }
          Token::Index { mode, .. } => {
            patch.index_mode = *mode;
          }
          Token::OldFile(file) => {
            patch.old_file = file;
          }
          Token::NewFile(file) => {
            patch.new_file = file;
          }
          Token::CopyFrom(from) => {
            patch.copy_from = Some(from);
          }
          Token::CopyTo(to) => {
            patch.copy_to = Some(to);
          }
          Token::RenameFrom(from) => {
            patch.rename_from = Some(from);
          }
          Token::RenameTo(to) => {
            patch.rename_to = Some(to);
          }
          Token::NewFileMode(mode) => {
            patch.new_mode = Some(*mode);
          }
          Token::OldFileMode(mode) => {
            patch.old_mode = Some(*mode);
          }
          Token::DeletedFileMode(mode) => {
            patch.deleted_mode = Some(*mode);
          }
          Token::Similarity(percent) => {
            patch.similarity = Some(*percent);
          }
          Token::Dissimilarity(p) => {
            patch.dissimilarity = Some(*p);
          }
          Token::Binary { old_file, new_file } => {
            patch.old_file = old_file;
            patch.new_file = new_file;
            patch.binary = true;
            self.tokens.next(); // Consume the `Binary` token.
            return Ok(()); // Binary patches have no hunks, so we're done.
          }
          // If the token is not a header token, we break the loop.
          _ => break,
        },
        // Propagate any parsing errors from the lexer.
        Some(Err(e)) => return Err(Error::Parse(e.clone())),
        // Stop if there are no more tokens.
        None => break,
      }
      // Consume the successfully processed header token.
      self.tokens.next();
    }
    Ok(())
  }

  fn parse_hunks(&mut self, patch: &mut Patch<'a>) -> Result<(), Error> {
    while self.peek_is(|t| matches!(t, Token::HunkHeader { .. }))? {
      let (hunk, new_file_no_newline) = self.parse_hunk()?;
      if new_file_no_newline {
        patch.new_file_no_newline = true;
      }
      patch.hunks.push(hunk);
    }
    Ok(())
  }

  fn parse_headerless_hunk(
    &mut self,
    patch: &mut Patch<'a>,
  ) -> Result<(), Error> {
    let mut lines = Vec::new();
    let (old_span, new_span, new_file_no_newline) =
      self.parse_hunk_lines(&mut lines)?;

    patch.new_file_no_newline = new_file_no_newline;

    if !lines.is_empty() {
      if patch.old_file.is_empty() && patch.new_file.is_empty() {
        return Err(Error::Parse(ParseError::PatchHasContentButNoFileInfo));
      }

      patch.hunks.push(Hunk {
        old_line: u32::from(old_span > 0),
        new_line: u32::from(new_span > 0),
        old_span,
        new_span,
        lines,
      });
    }
    Ok(())
  }

  fn parse_patch(&mut self) -> Result<Patch<'a>, Error> {
    let mut patch = Patch::default();
    self.parse_header(&mut patch)?;
    self.skip_empty_context_lines();
    self.parse_hunks(&mut patch)?;

    if patch.hunks.is_empty() {
      self.parse_headerless_hunk(&mut patch)?;
    }

    Ok(patch)
  }

  fn parse_hunk_lines(
    &mut self,
    lines: &mut Vec<Line<'a>>,
  ) -> Result<(u32, u32, bool), Error> {
    let mut old_span = 0;
    let mut new_span = 0;
    let mut last_line_was_new_file = false;
    let mut new_file_no_newline = false;

    while let Some(Ok(token)) = self.tokens.peek() {
      match token {
        Token::Addition(s) => {
          new_span += 1;
          lines.push(Line::Addition(s));
          last_line_was_new_file = true;
        }
        Token::Deletion(s) => {
          old_span += 1;
          lines.push(Line::Deletion(s));
          last_line_was_new_file = false;
        }
        Token::Context(s) => {
          old_span += 1;
          new_span += 1;
          lines.push(Line::Context(s));
          last_line_was_new_file = true;
        }
        Token::NoNewline => {
          if last_line_was_new_file {
            new_file_no_newline = true;
          }
        }
        _ => break,
      }
      self.tokens.next();
    }
    Ok((old_span, new_span, new_file_no_newline))
  }

  fn parse_hunk(&mut self) -> Result<(Hunk<'a>, bool), Error> {
    let token = self.tokens.next().ok_or(ParseError::UnexpectedEof)??;
    let (old_line, expected_old_span, new_line, expected_new_span) =
      if let Token::HunkHeader {
        old_line,
        old_span,
        new_line,
        new_span,
      } = token
      {
        (old_line, old_span, new_line, new_span)
      } else {
        return Err(Error::Parse(ParseError::ExpectedHunkHeader));
      };

    let mut lines =
      Vec::with_capacity((expected_old_span + expected_new_span) as usize);
    let (old_span, new_span, new_file_no_newline) =
      self.parse_hunk_lines(&mut lines)?;

    if old_span != expected_old_span {
      return Err(Error::Parse(ParseError::HunkLineCountMismatchOld {
        expected: expected_old_span,
        actual: old_span,
      }));
    }

    if new_span != expected_new_span {
      return Err(Error::Parse(ParseError::HunkLineCountMismatchNew {
        expected: expected_new_span,
        actual: new_span,
      }));
    }

    let hunk = Hunk {
      old_line,
      old_span,
      new_line,
      new_span,
      lines,
    };

    Ok((hunk, new_file_no_newline))
  }

  fn skip_empty_context_lines(&mut self) {
    while self
      .peek_is(|t| matches!(t, Token::Context(s) if s.is_empty()))
      .unwrap_or(false)
    {
      self.tokens.next();
    }
  }

  fn peek_is(
    &mut self,
    check: impl Fn(&Token<'a>) -> bool,
  ) -> Result<bool, Error> {
    match self.tokens.peek() {
      Some(Ok(token)) => Ok(check(token)),
      Some(Err(e)) => Err(Error::Parse(e.clone())),
      None => Ok(false),
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
          && patch.hunks.is_empty() =>
      {
        None
      }
      res => Some(res),
    }
  }
}
