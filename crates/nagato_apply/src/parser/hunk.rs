use nagato_core::{parse_int, Error, ErrorKind};

use crate::{Hunk, LexerItem, Line, LineKind, Parser, Patch, TokenKind};

pub fn parse_hunks<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
  hunks: &mut Vec<Hunk<'a>>,
) -> Result<(), Error> {
  loop {
    parser.skip_empty_context_lines()?;

    let mut found_something = false;

    if parser.peek_is(|t| matches!(t, TokenKind::Label(..)))? {
      if let Some(Ok(item)) = parser.tokens.next() {
        if let TokenKind::Label(l) = item.token {
          parser.label = Some(l);
        }
      }
      found_something = true;
      parser.skip_empty_context_lines()?;
    }

    if parser.peek_is(|t| matches!(t, TokenKind::HunkHeader { .. }))? {
      let mut hunk = parse_hunk(parser, patch)?;
      if hunk.label.is_none() {
        hunk.label = parser.label.take();
      } else {
        parser.label = None;
      }
      hunks.push(hunk);
      found_something = true;
    } else if parser.peek_is(|t| {
      matches!(
        t,
        TokenKind::Addition(..)
          | TokenKind::Deletion(..)
          | TokenKind::Context(..)
      )
    })? {
      let initial_item = parser
        .tokens
        .peek()
        .and_then(|res| res.as_ref().ok())
        .ok_or(Error::new(ErrorKind::UnexpectedEof))?;
      let initial_line = initial_item.line_num;

      let mut lines = Vec::new();
      let (old_span, new_span) =
        collect_hunk_lines(parser, &mut lines, patch, true)?;

      if !lines.is_empty() {
        hunks.push(Hunk {
          old_line: 0,
          new_line: 0,
          old_span,
          new_span,
          lines: lines.into_boxed_slice(),
          patch_line_num: initial_line.saturating_sub(1),
          has_header: false,
          label: parser.label.take(),
        });
        found_something = true;
      }
    }

    if !found_something {
      break;
    }
  }
  Ok(())
}

/// Collects hunk lines from the parser and updates the patch metadata.
/// Returns the (old_span, new_span) of the collected lines.
/// If `stop_on_empty` is true, stops at empty context lines (used for hunkless).
pub fn collect_hunk_lines<'a>(
  parser: &mut Parser<'a>,
  lines: &mut Vec<Line<'a>>,
  patch: &mut Patch<'a>,
  stop_on_empty: bool,
) -> Result<(u32, u32), Error> {
  let mut old_span = 0;
  let mut new_span = 0;

  while let Some(res) = parser.tokens.peek() {
    let item = match res {
      Ok(i) => i,
      Err(_) => return Err(parser.tokens.next().unwrap().unwrap_err()),
    };

    if stop_on_empty
      && matches!(item.token, TokenKind::Context(s) if s.is_empty())
    {
      break;
    }

    let line = match &item.token {
      TokenKind::Addition(text) => {
        new_span += 1;
        Some(Line {
          kind: LineKind::Addition,
          text,
        })
      }
      TokenKind::Deletion(text) => {
        old_span += 1;
        Some(Line {
          kind: LineKind::Deletion,
          text,
        })
      }
      TokenKind::Context(text) => {
        old_span += 1;
        new_span += 1;
        Some(Line {
          kind: LineKind::Context,
          text,
        })
      }
      _ => None,
    };

    if let Some(line) = line {
      lines.push(line);
      parser.tokens.next();
      continue;
    }

    match &item.token {
      TokenKind::OldFileNoNewline => patch.old_file_no_newline = true,
      TokenKind::NewFileNoNewline => patch.new_file_no_newline = true,
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
  let (old_line, old_span, new_line, new_span, label, patch_line_num) =
    match parser
      .tokens
      .next()
      .ok_or(Error::new(ErrorKind::UnexpectedEof))??
    {
      LexerItem {
        token:
          TokenKind::HunkHeader {
            old_range,
            new_range,
            label,
          },
        line_num,
      } => {
        let (old_line, old_span) =
          parse_range(old_range).map_err(|k| Error::with_line(k, line_num))?;
        let (new_line, new_span) =
          parse_range(new_range).map_err(|k| Error::with_line(k, line_num))?;
        (old_line, old_span, new_line, new_span, label, line_num)
      }
      item => {
        return Err(Error::with_line(
          ErrorKind::ExpectedHunkHeader,
          item.line_num,
        ))
      }
    };

  let mut lines = Vec::with_capacity(old_span.max(new_span) as usize);
  let (actual_old_span, actual_new_span) =
    collect_hunk_lines(parser, &mut lines, patch, false)?;

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
    lines: lines.into_boxed_slice(),
    patch_line_num,
    has_header: true,
    label,
  })
}

pub fn parse_hunkless<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
  hunks: &mut Vec<Hunk<'a>>,
) -> Result<(), Error> {
  let initial_start_line = parser
    .tokens
    .peek()
    .and_then(|res| res.as_ref().ok())
    .map(|item| item.line_num)
    .unwrap_or(0);

  parse_hunks(parser, patch, hunks)?;

  if !hunks.is_empty() && patch.old_file.is_empty() && patch.new_file.is_empty()
  {
    return Err(Error::with_line(
      ErrorKind::PatchHasContentButNoFileInfo,
      initial_start_line,
    ));
  }
  Ok(())
}

fn parse_range(range_bytes: &[u8]) -> Result<(u32, u32), ErrorKind> {
  let (line, rest) =
    parse_int::<u32>(range_bytes, 10).ok_or(ErrorKind::InvalidHunkRangeLine)?;

  let span = if let Some(rest) = rest.strip_prefix(b",") {
    let (span, rest) =
      parse_int::<u32>(rest, 10).ok_or(ErrorKind::InvalidHunkRangeSpan)?;
    if !rest.is_empty() {
      return Err(ErrorKind::InvalidHunkRangeSpan);
    }
    span
  } else if rest.is_empty() {
    1
  } else {
    return Err(ErrorKind::InvalidHunkRangeLine);
  };

  Ok((line, span))
}
