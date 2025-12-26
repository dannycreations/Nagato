use std::mem;

use nagato_core::{Error, ErrorKind};

use crate::{Hunk, LexerItem, Line, LineKind, Parser, Patch, TokenKind};

pub fn parse_hunks<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<(), Error> {
  while parser.peek_is(|t| matches!(t, TokenKind::HunkHeader { .. })) {
    let hunk = parse_hunk(parser, patch)?;
    patch.hunks.push(hunk);
  }
  Ok(())
}

pub fn parse_hunk_lines<'a>(
  parser: &mut Parser<'a>,
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
  parser: &mut Parser<'a>,
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
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<(), Error> {
  let initial_start_line = parser
    .tokens
    .peek()
    .and_then(|res| res.as_ref().ok())
    .map(|item| item.line_num)
    .unwrap_or(0);
  let mut current_lines: Vec<Line<'a>> = Vec::new();
  let mut hunk_start_line = initial_start_line;

  while let Some(Ok(item)) = parser.tokens.peek() {
    let token = &item.token;

    if matches!(token, TokenKind::Context(s) if s.is_empty()) {
      if !current_lines.is_empty() {
        patch.hunks.push(Hunk {
          old_line: 0,
          new_line: 0,
          old_span: current_lines
            .iter()
            .filter(|l| !matches!(l.kind, LineKind::Addition))
            .count() as u32,
          new_span: current_lines
            .iter()
            .filter(|l| !matches!(l.kind, LineKind::Deletion))
            .count() as u32,
          lines: mem::take(&mut current_lines),
          patch_line_num: hunk_start_line.saturating_sub(1),
        });
        parser.tokens.next();
        if let Some(Ok(next_item)) = parser.tokens.peek() {
          hunk_start_line = next_item.line_num;
        }
        continue;
      } else {
        parser.tokens.next();
        if let Some(Ok(next_item)) = parser.tokens.peek() {
          hunk_start_line = next_item.line_num;
        }
        continue;
      }
    }

    match token {
      TokenKind::Addition(text) => current_lines.push(Line {
        kind: LineKind::Addition,
        text,
      }),
      TokenKind::Deletion(text) => current_lines.push(Line {
        kind: LineKind::Deletion,
        text,
      }),
      TokenKind::Context(text) => current_lines.push(Line {
        kind: LineKind::Context,
        text,
      }),
      TokenKind::OldFileNoNewline => patch.old_file_no_newline = true,
      TokenKind::NewFileNoNewline => patch.new_file_no_newline = true,
      _ => break,
    }
    parser.tokens.next();
  }

  if !current_lines.is_empty() {
    patch.hunks.push(Hunk {
      old_line: 0,
      new_line: 0,
      old_span: current_lines
        .iter()
        .filter(|l| !matches!(l.kind, LineKind::Addition))
        .count() as u32,
      new_span: current_lines
        .iter()
        .filter(|l| !matches!(l.kind, LineKind::Deletion))
        .count() as u32,
      lines: current_lines,
      patch_line_num: hunk_start_line.saturating_sub(1),
    });
  }

  if !patch.hunks.is_empty()
    && patch.old_file.is_empty()
    && patch.new_file.is_empty()
  {
    return Err(Error::with_line(
      ErrorKind::PatchHasContentButNoFileInfo,
      initial_start_line,
    ));
  }
  Ok(())
}
