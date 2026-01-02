use nagato_core::{parse_int, Error, ErrorKind};

use crate::{Hunk, Line, LineKind, Parser, Patch, TokenKind};

pub fn parse_hunks<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
  hunks: &mut Vec<Hunk<'a>>,
) -> Result<(), Error> {
  while let Some(item) = parser.peek_token()? {
    match &item.token {
      TokenKind::Label(l) => {
        parser.label = Some(*l);
        parser.tokens.next();
        parser.skip_empty_context_lines()?;
        continue;
      }
      TokenKind::HunkHeader { .. } => {
        let mut hunk = parse_hunk(parser, patch)?;
        hunk.label = hunk.label.or_else(|| parser.label.take());
        hunks.push(hunk);
      }
      TokenKind::Addition(_)
      | TokenKind::Deletion(_)
      | TokenKind::Context(_) => {
        let initial_line = item.line_num;
        let mut lines = Vec::new();
        let (old_span, new_span) = collect_hunk_lines(
          parser,
          &mut lines,
          patch,
          |t| matches!(t, TokenKind::Context(s) if s.is_empty()),
        )?;

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
        }
      }
      _ => break,
    }
    parser.skip_empty_context_lines()?;
  }
  Ok(())
}

/// Collects hunk lines from the parser and updates the patch metadata.
/// Returns the (old_span, new_span) of the collected lines.
/// The `stop_condition` closure allows customizing when to stop collecting lines.
pub fn collect_hunk_lines<'a>(
  parser: &mut Parser<'a>,
  lines: &mut Vec<Line<'a>>,
  patch: &mut Patch<'a>,
  stop_condition: impl Fn(&TokenKind<'a>) -> bool,
) -> Result<(u32, u32), Error> {
  let mut old_span = 0;
  let mut new_span = 0;

  while let Some(item) = parser.peek_token()? {
    if stop_condition(&item.token) {
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
  let item = parser
    .tokens
    .next()
    .ok_or(Error::new(ErrorKind::UnexpectedEof))??;

  let (old_range, new_range, label) = match item.token {
    TokenKind::HunkHeader {
      old_range,
      new_range,
      label,
    } => (old_range, new_range, label),
    _ => {
      return Err(Error::with_line(
        ErrorKind::ExpectedHunkHeader,
        item.line_num,
      ))
    }
  };

  let (old_line, old_span) =
    parse_range(old_range).map_err(|k| Error::with_line(k, item.line_num))?;
  let (new_line, new_span) =
    parse_range(new_range).map_err(|k| Error::with_line(k, item.line_num))?;

  let mut lines = Vec::with_capacity(old_span.max(new_span) as usize);
  let (actual_old_span, actual_new_span) =
    collect_hunk_lines(parser, &mut lines, patch, |_| false)?;

  if actual_old_span != old_span || actual_new_span != new_span {
    return Err(Error::with_line(
      ErrorKind::HunkLineCountMismatch,
      item.line_num,
    ));
  }

  Ok(Hunk {
    old_line,
    old_span,
    new_line,
    new_span,
    lines: lines.into_boxed_slice(),
    patch_line_num: item.line_num,
    has_header: true,
    label,
  })
}

/// Parse a range in the format "line[,span]".
fn parse_range(range_bytes: &[u8]) -> Result<(u32, u32), ErrorKind> {
  let (line, rest) =
    parse_int::<u32>(range_bytes, 10).ok_or(ErrorKind::InvalidHunkRange)?;

  if rest.is_empty() {
    return Ok((line, 1));
  }

  let rest = rest.strip_prefix(b",").ok_or(ErrorKind::InvalidHunkRange)?;
  let (span, rest) =
    parse_int::<u32>(rest, 10).ok_or(ErrorKind::InvalidHunkRange)?;

  if !rest.is_empty() {
    return Err(ErrorKind::InvalidHunkRange);
  }

  Ok((line, span))
}
