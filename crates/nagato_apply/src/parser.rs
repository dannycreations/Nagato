use std::iter::Peekable;

use nagato_core::error::{Error, ErrorKind};

use crate::{Hunk, Lexer, LexerItem, Line, LineKind, Patch, TokenKind};

pub struct Parser<'a> {
  // The parser now consumes `LexerItem`s, giving it access to line numbers for all tokens.
  tokens: Peekable<Lexer<'a>>,
}

impl<'a> Parser<'a> {
  pub fn new(input: &'a [u8]) -> Self {
    Self {
      tokens: Lexer::new(input).peekable(),
    }
  }

  fn parse_header(&mut self, patch: &mut Patch<'a>) -> Result<(), Error> {
    // The logic now correctly peeks at `LexerItem.token` to decide whether to continue parsing the header.
    while let Some(Ok(item)) = self.tokens.peek() {
      match &item.token {
        TokenKind::FileHeader { old_file, new_file } => {
          patch.old_file = old_file;
          patch.new_file = new_file;
        }
        TokenKind::Index { mode, .. } => {
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
          self.tokens.next(); // Consume the `Binary` token.
          return Ok(()); // Binary patches have no hunks, so we're done.
        }
        _ => break,
      }
      self.tokens.next();
    }
    Ok(())
  }

  fn parse_hunks(&mut self, patch: &mut Patch<'a>) -> Result<(), Error> {
    while self.peek_is(|t| matches!(t, TokenKind::HunkHeader { .. }))? {
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
  ) -> Result<(), Error> {
    // We now get the starting line number from the first token of the hunk.
    // This is crucial for correctly reporting errors in headerless patches.
    let start_line_num = self
      .tokens
      .peek()
      .and_then(|res: &Result<LexerItem, Error>| res.as_ref().ok())
      .map(|item| item.line_num)
      .unwrap_or(0);

    let mut lines = Vec::new();
    let (old_span, new_span, old_no_newline, new_no_newline) =
      self.parse_hunk_lines(&mut lines)?;

    patch.old_file_no_newline = old_no_newline;
    patch.new_file_no_newline = new_no_newline;

    if !lines.is_empty() {
      if patch.old_file.is_empty() && patch.new_file.is_empty() {
        return Err(Error {
          line: Some(start_line_num),
          kind: ErrorKind::PatchHasContentButNoFileInfo,
        });
      }

      patch.hunks.push(Hunk {
        old_line: u32::from(old_span > 0),
        new_line: u32::from(new_span > 0),
        old_span,
        new_span,
        lines,
        // The line number is now correctly sourced from the first line of the hunk content.
        patch_line_num: start_line_num,
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
  ) -> Result<(u32, u32, bool, bool), Error> {
    let mut old_span = 0;
    let mut new_span = 0;
    let mut last_line_was_new_file = false;
    let mut old_file_no_newline = false;
    let mut new_file_no_newline = false;

    while let Some(Ok(item)) = self.tokens.peek() {
      let line_num = item.line_num;
      match &item.token {
        TokenKind::Addition(s) => {
          new_span += 1;
          lines.push(Line {
            kind: LineKind::Addition,
            text: s,
            line_num,
          });
          last_line_was_new_file = true;
        }
        TokenKind::Deletion(s) => {
          old_span += 1;
          lines.push(Line {
            kind: LineKind::Deletion,
            text: s,
            line_num,
          });
          last_line_was_new_file = false;
        }
        TokenKind::Context(s) => {
          old_span += 1;
          new_span += 1;
          lines.push(Line {
            kind: LineKind::Context,
            text: s,
            line_num,
          });
          last_line_was_new_file = true;
        }
        TokenKind::NoNewline => {
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

  fn parse_hunk(&mut self) -> Result<(Hunk<'a>, bool, bool), Error> {
    // The parser now consumes a `LexerItem` and extracts both the token and the line number.
    let (old_line, old_span, new_line, new_span, patch_line_num) =
      match self.tokens.next().ok_or(Error {
        line: None,
        kind: ErrorKind::UnexpectedEof,
      })?? {
        LexerItem {
          token:
            TokenKind::HunkHeader {
              old_line,
              old_span,
              new_line,
              new_span,
            },
          line_num,
        } => (old_line, old_span, new_line, new_span, line_num),
        item => {
          return Err(Error {
            line: Some(item.line_num),
            kind: ErrorKind::ExpectedHunkHeader,
          })
        }
      };

    let mut lines = Vec::with_capacity((old_span + new_span) as usize);
    let (
      actual_old_span,
      actual_new_span,
      old_file_no_newline,
      new_file_no_newline,
    ) = self.parse_hunk_lines(&mut lines)?;

    if actual_old_span != old_span || actual_new_span != new_span {
      return Err(Error {
        line: Some(patch_line_num),
        kind: ErrorKind::HunkLineCountMismatch,
      });
    }

    let hunk = Hunk {
      old_line,
      old_span,
      new_line,
      new_span,
      lines,
      patch_line_num,
    };

    Ok((hunk, old_file_no_newline, new_file_no_newline))
  }

  fn skip_empty_context_lines(&mut self) {
    while self
      .peek_is(|t| matches!(t, TokenKind::Context(s) if s.is_empty()))
      .unwrap_or(false)
    {
      self.tokens.next();
    }
  }

  fn peek_is(
    &mut self,
    check: impl Fn(&TokenKind<'a>) -> bool,
  ) -> Result<bool, Error> {
    // The `peek_is` function now correctly looks inside the `LexerItem` to check the token.
    match self.tokens.peek() {
      Some(Ok(item)) => Ok(check(&item.token)),
      Some(Err(e)) => Err(e.clone()),
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
