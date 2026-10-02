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
        let (old_span, new_span) = collect_hunk_lines(parser, patch, true)?;
        let lines_len = patch.lines.len() as u32 - lines_start;

        if lines_len > 0 {
          Some(Hunk {
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

pub fn collect_hunk_lines<'a>(
  parser: &mut Parser<'a>,
  patch: &mut Patch<'a>,
  stop_on_padding: bool,
) -> Result<(u32, u32), Error> {
  let mut old_span = 0;
  let mut new_span = 0;

  while let Some(item) = parser.peek_token()? {
    if stop_on_padding && item.token.is_padding() {
      break;
    }

    let (kind, text) = match &item.token {
      TokenKind::Addition(text) => (LineKind::Addition, *text),
      TokenKind::Deletion(text) => (LineKind::Deletion, *text),
      TokenKind::Context(text) => (LineKind::Context, *text),
      TokenKind::Gap => (LineKind::Gap, &[][..]),
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
        continue;
      }
      _ => break,
    };

    // A line that contributes to one file version resets that side's
    // no-newline flag; only a trailing marker may set it again.
    if kind != LineKind::Addition {
      old_span += 1;
      patch.old_file_no_newline = false;
    }
    if kind != LineKind::Deletion {
      new_span += 1;
      patch.new_file_no_newline = false;
    }
    patch.lines.push(Line { kind, text });
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

  let old_span =
    parse_range(old_range).map_err(|k| Error::with_line(k, item.line_num))?;
  let new_span =
    parse_range(new_range).map_err(|k| Error::with_line(k, item.line_num))?;

  let lines_start = patch.lines.len() as u32;

  // reserve capacity in patch.lines to avoid reallocations on the hot path
  patch.lines.reserve(new_span.max(old_span) as usize);

  let (actual_old_span, actual_new_span) =
    match collect_hunk_lines(parser, patch, false) {
      Ok(spans) => spans,
      Err(e) => {
        patch.lines.truncate(lines_start as usize);
        return Err(e);
      }
    };

  if actual_old_span != old_span || actual_new_span != new_span {
    patch.lines.truncate(lines_start as usize);
    return Err(Error::with_line(
      ErrorKind::HunkLineCountMismatch,
      item.line_num,
    ));
  }

  let lines_len = patch.lines.len() as u32 - lines_start;

  Ok(Hunk {
    old_span,
    new_span,
    lines_start,
    lines_len,
    patch_line_num: item.line_num,
    has_header: true,
    label,
  })
}

fn parse_range(range_bytes: &[u8]) -> Result<u32, ErrorKind> {
  // Only the span drives application; the line number is still parsed so
  // malformed ranges such as "-x,2" keep failing here.
  let (line_part, span_part) = match memchr(b',', range_bytes) {
    Some(i) => (&range_bytes[..i], Some(&range_bytes[i + 1..])),
    None => (range_bytes, None),
  };

  parse_int::<u32>(line_part, 10).ok_or(ErrorKind::InvalidHunkRange)?;

  match span_part {
    Some(bytes) => {
      let (span, _) =
        parse_int::<u32>(bytes, 10).ok_or(ErrorKind::InvalidHunkRange)?;
      Ok(span)
    }
    None => Ok(1),
  }
}
