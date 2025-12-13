use std::iter::Peekable;

use nagato_core::error::ErrorKind;

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

  fn parse_header(&mut self, patch: &mut Patch<'a>) -> Result<(), ErrorKind> {
    // This refactoring simplifies the header parsing loop by using `while let`,
    // which is more idiomatic and readable than the previous `loop` and `match` combination.
    while let Some(Ok(token)) = self.tokens.peek() {
      match token {
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
      }
      self.tokens.next();
    }
    Ok(())
  }

  fn parse_hunks(&mut self, patch: &mut Patch<'a>) -> Result<(), ErrorKind> {
    while self.peek_is(|t| matches!(t, Token::HunkHeader { .. }))? {
      let (hunk, old_no_newline, new_no_newline) = self.parse_hunk()?;
      if old_no_newline {
        patch.old_file_no_newline = true;
      }
      if new_no_newline {
        patch.new_file_no_newline = true;
      }
      patch.hunks.push(hunk);
    }
    Ok(())
  }

  fn parse_headerless_hunk(
    &mut self,
    patch: &mut Patch<'a>,
  ) -> Result<(), ErrorKind> {
    let mut lines = Vec::new();
    let (old_span, new_span, old_no_newline, new_no_newline) =
      self.parse_hunk_lines(&mut lines)?;

    patch.old_file_no_newline = old_no_newline;
    patch.new_file_no_newline = new_no_newline;

    if !lines.is_empty() {
      if patch.old_file.is_empty() && patch.new_file.is_empty() {
        return Err(ErrorKind::PatchHasContentButNoFileInfo);
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

  fn parse_patch(&mut self) -> Result<Patch<'a>, ErrorKind> {
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
  ) -> Result<(u32, u32, bool, bool), ErrorKind> {
    let mut old_span = 0;
    let mut new_span = 0;
    let mut last_line_was_new_file = false;
    let mut old_file_no_newline = false;
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
          } else {
            old_file_no_newline = true;
          }
        }
        _ => break,
      }
      self.tokens.next();
    }
    Ok((old_span, new_span, old_file_no_newline, new_file_no_newline))
  }

  fn parse_hunk(&mut self) -> Result<(Hunk<'a>, bool, bool), ErrorKind> {
    // This was refactored to use a `match` expression, which is more idiomatic
    // for this kind of token processing. It makes the intent clearer.
    let (old_line, old_span, new_line, new_span) =
      match self.tokens.next().ok_or(ErrorKind::UnexpectedEof)?? {
        Token::HunkHeader {
          old_line,
          old_span,
          new_line,
          new_span,
        } => (old_line, old_span, new_line, new_span),
        _ => return Err(ErrorKind::ExpectedHunkHeader),
      };

    let mut lines = Vec::with_capacity((old_span + new_span) as usize);
    // Renamed variables to `actual_...` to distinguish them from the
    // expected spans from the hunk header, improving clarity.
    let (
      actual_old_span,
      actual_new_span,
      old_file_no_newline,
      new_file_no_newline,
    ) = self.parse_hunk_lines(&mut lines)?;

    if actual_old_span != old_span || actual_new_span != new_span {
      return Err(ErrorKind::HunkLineCountMismatch);
    }

    let hunk = Hunk {
      old_line,
      old_span,
      new_line,
      new_span,
      lines,
    };

    Ok((hunk, old_file_no_newline, new_file_no_newline))
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
  ) -> Result<bool, ErrorKind> {
    match self.tokens.peek() {
      Some(Ok(token)) => Ok(check(token)),
      Some(Err(e)) => Err(e.clone()),
      None => Ok(false),
    }
  }
}

impl<'a> Iterator for Parser<'a> {
  type Item = Result<Patch<'a>, ErrorKind>;

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
