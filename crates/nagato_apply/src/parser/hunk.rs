use memchr::memchr;
use nagato_core::{parse_int, Error, ErrorKind};

use crate::{Hunk, Line, LineKind, Parser, Patch, TokenKind};

pub fn next_hunk<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<Option<Hunk<'a>>, Error> {
  while let Some(item) = parser.peek_token()? {
    let res = match &item.token {
      TokenKind::Label(l) => {
        parser.label = Some(*l);
        parser.tokens.next();
        parser.skip_empty_context_lines()?;
        continue;
      }
      TokenKind::HunkHeader { .. } => {
        let mut hunk = parse_hunk(parser, patch)?;
        hunk.label = hunk.label.or_else(|| parser.label.take());
        Some(hunk)
      }
      TokenKind::Addition(_)
      | TokenKind::Deletion(_)
      | TokenKind::Context(_)
      | TokenKind::Gap => {
        let initial_line = item.line_num;
        let lines_start = patch.lines.len() as u32;
        let (old_span, new_span) = collect_hunk_lines(parser, patch, |t| {
          matches!(t, TokenKind::Gap)
            || matches!(t, TokenKind::Context(s) if s.is_empty())
        })?;
        let lines_len = patch.lines.len() as u32 - lines_start;

        if lines_len > 0 {
          Some(Hunk {
            old_line: 0,
            new_line: 0,
            old_span,
            new_span,
            lines_start,
            lines_len,
            patch_line_num: initial_line.saturating_sub(1),
            has_header: false,
            label: parser.label.take(),
          })
        } else {
          None
        }
      }
      _ => return Ok(None),
    };
    parser.skip_empty_context_lines()?;
    if res.is_some() {
      return Ok(res);
    }
  }
  Ok(None)
}

pub fn parse_hunks<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
) -> Result<(), Error> {
  while let Some(hunk) = next_hunk(parser, patch)? {
    patch.hunks.push(hunk);
  }
  Ok(())
}

pub fn collect_hunk_lines<'a>(
  parser: &mut Parser<'a>,
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
    match &item.token {
      TokenKind::Addition(text) => {
        new_span += 1;
        patch.new_file_no_newline = false;
        patch.lines.push(Line {
          kind: LineKind::Addition,
          text,
        });
        parser.tokens.next();
      }
      TokenKind::Deletion(text) => {
        old_span += 1;
        patch.old_file_no_newline = false;
        patch.lines.push(Line {
          kind: LineKind::Deletion,
          text,
        });
        parser.tokens.next();
      }
      TokenKind::Context(text) => {
        old_span += 1;
        new_span += 1;
        patch.old_file_no_newline = false;
        patch.new_file_no_newline = false;
        patch.lines.push(Line {
          kind: LineKind::Context,
          text,
        });
        parser.tokens.next();
      }
      TokenKind::Gap => {
        old_span += 1;
        new_span += 1;
        patch.old_file_no_newline = false;
        patch.new_file_no_newline = false;
        patch.lines.push(Line {
          kind: LineKind::Gap,
          text: &[],
        });
        parser.tokens.next();
      }
      TokenKind::NoNewline => {
        parser.tokens.next();
        let Some(last) = patch.lines.last() else {
          continue;
        };

        if new_span > 0 && last.kind != LineKind::Deletion {
          patch.new_file_no_newline = true;
        }
        if old_span > 0 && last.kind != LineKind::Addition {
          patch.old_file_no_newline = true;
        }
      }
      _ => break,
    };
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

  let lines_start = patch.lines.len() as u32;

  // reserve capacity in patch.lines to avoid reallocations on the hot path
  patch.lines.reserve(new_span.max(old_span) as usize);

  let (actual_old_span, actual_new_span) =
    match collect_hunk_lines(parser, patch, |_| false) {
      Ok(spans) => spans,
      Err(e) => {
        patch.lines.truncate(lines_start as usize);
        return Err(e);
      }
    };

  // Hunk integrity is verified by comparing the actual line counts accumulated during parsing against the expected spans declared in the hunk header.
  if actual_old_span != old_span || actual_new_span != new_span {
    patch.lines.truncate(lines_start as usize);
    return Err(Error::with_line(
      ErrorKind::HunkLineCountMismatch,
      item.line_num,
    ));
  }

  let lines_len = patch.lines.len() as u32 - lines_start;

  Ok(Hunk {
    old_line,
    old_span,
    new_line,
    new_span,
    lines_start,
    lines_len,
    patch_line_num: item.line_num,
    has_header: true,
    label,
  })
}

fn parse_range(range_bytes: &[u8]) -> Result<(u32, u32), ErrorKind> {
  let idx = match memchr(b',', range_bytes) {
    Some(i) => i,
    None => {
      let (line, _) =
        parse_int::<u32>(range_bytes, 10).ok_or(ErrorKind::InvalidHunkRange)?;
      return Ok((line, 1));
    }
  };

  let line_part = &range_bytes[..idx];
  let span_part = &range_bytes[idx + 1..];

  let (line, _) =
    parse_int::<u32>(line_part, 10).ok_or(ErrorKind::InvalidHunkRange)?;
  let (span, _) =
    parse_int::<u32>(span_part, 10).ok_or(ErrorKind::InvalidHunkRange)?;

  Ok((line, span))
}
