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
        // Labels from `label ` lines apply to the next hunk.
        // If the hunk has its own label in the `@@` header, it takes precedence.
        hunk.label = hunk.label.or_else(|| parser.label.take());
        hunks.push(hunk);
      }
      TokenKind::Addition(_)
      | TokenKind::Deletion(_)
      | TokenKind::Context(_)
      | TokenKind::Gap => {
        let initial_line = item.line_num;
        let mut lines = Vec::new();
        let (old_span, new_span) =
          collect_hunk_lines(parser, &mut lines, patch, |t| {
            matches!(t, TokenKind::Gap)
              || matches!(t, TokenKind::Context(s) if s.is_empty())
          })?;

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

    // Hunk line processing resets the no-newline flags to ensure that markers only apply when they are the final elements for their respective file versions.
    let line = match &item.token {
      TokenKind::Addition(text) => {
        new_span += 1;
        patch.new_file_no_newline = false;
        Some(Line {
          kind: LineKind::Addition,
          text,
        })
      }
      TokenKind::Deletion(text) => {
        old_span += 1;
        patch.old_file_no_newline = false;
        Some(Line {
          kind: LineKind::Deletion,
          text,
        })
      }
      TokenKind::Context(text) => {
        old_span += 1;
        new_span += 1;
        patch.old_file_no_newline = false;
        patch.new_file_no_newline = false;
        Some(Line {
          kind: LineKind::Context,
          text,
        })
      }
      TokenKind::Gap => {
        old_span += 1;
        new_span += 1;
        patch.old_file_no_newline = false;
        patch.new_file_no_newline = false;
        Some(Line {
          kind: LineKind::Gap,
          text: &[],
        })
      }
      TokenKind::NoNewline => {
        let is_new = new_span > 0
          && lines.last().is_some_and(|l| l.kind != LineKind::Deletion);
        if is_new {
          patch.new_file_no_newline = true;
        }

        let is_old = old_span > 0
          && lines.last().is_some_and(|l| l.kind != LineKind::Addition);
        if is_old {
          patch.old_file_no_newline = true;
        }
        None
      }
      _ => break,
    };

    parser.tokens.next();
    if let Some(line) = line {
      lines.push(line);
    }
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

  // Hunk integrity is verified by comparing the actual line counts accumulated during parsing against the expected spans declared in the hunk header.
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
    lines: lines.into(),
    patch_line_num: item.line_num,
    has_header: true,
    label,
  })
}

fn parse_range(range_bytes: &[u8]) -> Result<(u32, u32), ErrorKind> {
  let (line_part, span_part) = match memchr::memchr(b',', range_bytes) {
    Some(idx) => (&range_bytes[..idx], &range_bytes[idx + 1..]),
    None => (range_bytes, &b"1"[..]),
  };

  let line = parse_int::<u32>(line_part, 10)
    .map(|(v, _)| v)
    .ok_or(ErrorKind::InvalidHunkRange)?;
  let span = parse_int::<u32>(span_part, 10)
    .map(|(v, _)| v)
    .ok_or(ErrorKind::InvalidHunkRange)?;

  Ok((line, span))
}
