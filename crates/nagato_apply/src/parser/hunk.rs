use nagato_core::error::{Error, ErrorKind};

use crate::{
  lexer::{LexerItem, TokenKind},
  models::{Hunk, Line, LineKind, Patch},
};

pub fn parse_hunks<'a>(
  parser: &mut crate::parser::Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<(), Error> {
  while parser.peek_is(|t| matches!(t, TokenKind::HunkHeader { .. })) {
    let hunk = parse_hunk(parser, patch)?;
    patch.hunks.push(hunk);
  }
  Ok(())
}

pub fn parse_hunk_lines<'a>(
  parser: &mut crate::parser::Parser<'a>,
  lines: &mut Vec<Line<'a>>,
  patch: &mut Patch<'a>,
) -> Result<(u32, u32), Error> {
  let mut old_span = 0;
  let mut new_span = 0;

  while let Some(Ok(item)) = parser.tokens.peek() {
    match &item.token {
      TokenKind::Addition(s) => {
        new_span += 1;
        lines.push(Line {
          kind: LineKind::Addition,
          text: s,
        });
      }
      TokenKind::Deletion(s) => {
        old_span += 1;
        lines.push(Line {
          kind: LineKind::Deletion,
          text: s,
        });
      }
      TokenKind::Context(s) => {
        old_span += 1;
        new_span += 1;
        lines.push(Line {
          kind: LineKind::Context,
          text: s,
        });
      }
      TokenKind::OldFileNoNewline => {
        patch.old_file_no_newline = true;
      }
      TokenKind::NewFileNoNewline => {
        patch.new_file_no_newline = true;
      }
      _ => break,
    }
    parser.tokens.next();
  }
  Ok((old_span, new_span))
}

pub fn parse_hunk<'a>(
  parser: &mut crate::parser::Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<Hunk<'a>, Error> {
  let (old_line, old_span, new_line, new_span, patch_line_num) = match parser
    .tokens
    .next()
    .ok_or(Error::new(ErrorKind::UnexpectedEof))??
  {
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
      return Err(Error::with_line(
        ErrorKind::ExpectedHunkHeader,
        item.line_num,
      ))
    }
  };

  let cap = (old_span as usize).max(new_span as usize);
  let mut lines = Vec::with_capacity(cap);
  let (actual_old_span, actual_new_span) =
    parse_hunk_lines(parser, &mut lines, patch)?;

  if actual_old_span != old_span || actual_new_span != new_span {
    return Err(Error::with_line(
      ErrorKind::HunkLineCountMismatch,
      patch_line_num,
    ));
  }

  Ok(Hunk {
    old_line,
    old_span,
    new_line,
    new_span,
    lines,
    patch_line_num,
  })
}

pub fn parse_headerless_hunk<'a>(
  parser: &mut crate::parser::Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<(), Error> {
  let start_line_num = parser
    .tokens
    .peek()
    .and_then(|res| res.as_ref().ok())
    .map(|item| item.line_num)
    .unwrap_or(0);

  let mut lines = Vec::new();
  let (old_span, new_span) = parse_hunk_lines(parser, &mut lines, patch)?;

  if !lines.is_empty() {
    if patch.old_file.is_empty() && patch.new_file.is_empty() {
      return Err(Error::with_line(
        ErrorKind::PatchHasContentButNoFileInfo,
        start_line_num,
      ));
    }

    patch.hunks.push(Hunk {
      old_line: u32::from(old_span > 0),
      new_line: u32::from(new_span > 0),
      old_span,
      new_span,
      lines,
      patch_line_num: start_line_num.saturating_sub(1),
    });
  }
  Ok(())
}
