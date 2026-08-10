use std::iter::from_fn;

use memchr::{
  memchr,
  memmem::{self, Finder},
  memrchr,
};
use nagato_core::{get_line, Error, ErrorKind};

use crate::{Hunk, Line, LineKind, Patch};

#[inline]
fn lines_to_match<'h, 'p>(
  lines: &'h [Line<'p>],
) -> impl Iterator<Item = (usize, &'h Line<'p>)> + Clone {
  lines
    .iter()
    .enumerate()
    .filter(|(_, l)| !matches!(l.kind, LineKind::Addition))
}

#[inline]
pub(crate) fn first_non_empty_match_line<'h, 'p>(
  lines: &'h [Line<'p>],
) -> Option<(usize, &'h Line<'p>)> {
  lines_to_match(lines).find(|(_, l)| !l.text.is_empty())
}

#[inline]
fn best_match_line<'h, 'p>(
  lines: &'h [Line<'p>],
) -> Option<(usize, &'h Line<'p>)> {
  lines_to_match(lines)
    .filter(|(_, l)| !l.text.is_empty())
    .max_by_key(|(_, l)| l.text.len())
}

#[derive(Default)]
struct BestError {
  error: Option<Error>,
  max_offset: u32,
}

impl BestError {
  #[inline]
  fn record(&mut self, e: Error, base_line: u32) {
    let offset = e.line.unwrap_or(0).saturating_sub(base_line);
    if offset > self.max_offset || self.error.is_none() {
      self.max_offset = offset;
      self.error = Some(e);
    }
  }

  fn into_error(self, hunk: &Hunk<'_>) -> Error {
    self.error.unwrap_or_else(|| {
      let line = hunk.patch_line_num + u32::from(!hunk.has_header);
      Error::with_line(ErrorKind::CouldNotApplyHunk, line)
    })
  }
}

fn needle_anchors<'a>(
  buffer: &'a [u8],
  search_buffer: &'a [u8],
  search_offset: usize,
  finder: &'a Finder<'_>,
) -> impl Iterator<Item = usize> + 'a {
  let needle_len = finder.needle().len();

  finder.find_iter(search_buffer).filter_map(move |found| {
    let anchor = search_offset + found;

    // The needle has to both start and end a line, otherwise it is only a
    // substring of some longer line rather than a line match.
    if anchor > 0 && buffer[anchor - 1] != b'\n' {
      return None;
    }
    if buffer.len() - anchor < needle_len {
      return None;
    }

    let after = buffer.get(anchor + needle_len..);
    matches!(
      after,
      Some([b'\n', ..]) | Some([b'\r', b'\n', ..]) | Some([]) | None
    )
    .then_some(anchor)
  })
}

fn empty_line_anchors(
  buffer: &[u8],
  search_offset: usize,
) -> impl Iterator<Item = usize> + '_ {
  let mut pos = search_offset;
  let mut done = false;

  from_fn(move || {
    while !done && pos <= buffer.len() {
      let remaining = &buffer[pos..];
      let Some((line, rest)) = get_line(remaining) else {
        break;
      };

      let consumed = remaining.len() - rest.len();
      let at_unterminated_end = rest.is_empty()
        && !remaining.is_empty()
        && remaining.last() != Some(&b'\n');

      if line.is_empty() {
        let anchor = pos;
        if at_unterminated_end {
          done = true;
        } else {
          pos += consumed;
        }
        return Some(anchor);
      }

      if at_unterminated_end {
        break;
      }
      pos += consumed;
    }

    done = true;
    None
  })
}

pub struct Matcher;

impl Matcher {
  #[inline]
  pub fn find_match<'s, 'p>(
    &self,
    buffer: &'s [u8],
    patch: &Patch<'p>,
    hunk: &Hunk<'p>,
    precomputed_finder: Option<&Finder<'_>>,
  ) -> Result<(usize, &'s [u8]), Error> {
    let lines = patch.hunk_lines(hunk);

    // Cheapest case first: the hunk already lines up at the current position.
    if let Ok(res) = self.verify_match(buffer, lines, hunk, 0, 0) {
      return Ok(res);
    }

    let hint = self.search_hint(buffer, hunk);

    // Only worth trying when the caller has not already chosen an anchor.
    if precomputed_finder.is_none() {
      if let Some((idx, line)) = best_match_line(lines) {
        let finder = Finder::new(line.text);
        let res = self.search(buffer, lines, hunk, idx, Some(&finder), hint);
        if res.is_ok() {
          return res;
        }
      }
    }

    let Some((anchor_idx, anchor_line)) = first_non_empty_match_line(lines)
    else {
      // Every matchable line is empty (gaps or blank context), so anchor on
      // the first one regardless of content.
      let Some((idx, _)) = lines_to_match(lines).next() else {
        return Ok((0, buffer));
      };
      return self.search(buffer, lines, hunk, idx, None, hint);
    };

    let owned_finder;
    let finder = match precomputed_finder {
      Some(finder) => finder,
      None => {
        owned_finder = Finder::new(anchor_line.text);
        &owned_finder
      }
    };

    self.search(buffer, lines, hunk, anchor_idx, Some(finder), hint)
  }

  fn search<'s, 'p>(
    &self,
    buffer: &'s [u8],
    lines: &[Line<'p>],
    hunk: &Hunk<'p>,
    anchor_line_idx: usize,
    finder: Option<&Finder<'_>>,
    (search_buffer, search_offset): (&'s [u8], usize),
  ) -> Result<(usize, &'s [u8]), Error> {
    match finder {
      Some(finder) => self.verify_anchors(
        buffer,
        lines,
        hunk,
        anchor_line_idx,
        needle_anchors(buffer, search_buffer, search_offset, finder),
      ),
      None => self.verify_anchors(
        buffer,
        lines,
        hunk,
        anchor_line_idx,
        empty_line_anchors(buffer, search_offset),
      ),
    }
  }

  fn verify_anchors<'s, 'p>(
    &self,
    buffer: &'s [u8],
    lines: &[Line<'p>],
    hunk: &Hunk<'p>,
    anchor_line_idx: usize,
    anchors: impl Iterator<Item = usize>,
  ) -> Result<(usize, &'s [u8]), Error> {
    let mut best = BestError::default();

    for anchor in anchors {
      match self.verify_match(buffer, lines, hunk, anchor, anchor_line_idx) {
        Ok(res) => return Ok(res),
        Err(e) => best.record(e, hunk.patch_line_num),
      }
    }

    Err(best.into_error(hunk))
  }

  fn search_hint<'s>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'_>,
  ) -> (&'s [u8], usize) {
    let Some(label) = hunk.label.filter(|l| !l.is_empty()) else {
      return (buffer, 0);
    };

    for pos in memmem::find_iter(buffer, label) {
      if pos != 0 && buffer[pos - 1] != b'\n' {
        continue;
      }

      let line_end = match memchr(b'\n', &buffer[pos..]) {
        Some(i) => pos + i + 1,
        None => buffer.len(),
      };
      return (&buffer[line_end..], line_end);
    }

    (buffer, 0)
  }

  #[inline]
  fn verify_match<'s, 'p>(
    &self,
    source: &'s [u8],
    lines: &[Line<'p>],
    hunk: &Hunk<'p>,
    anchor_pos: usize,
    anchor_line_idx: usize,
  ) -> Result<(usize, &'s [u8]), Error> {
    let base_line = hunk.patch_line_num;
    let fail = |i: usize| {
      Err(Error::with_line(
        ErrorKind::CouldNotApplyHunk,
        base_line + 1 + i as u32,
      ))
    };

    // Walk backwards from the anchor over the preceding matchable lines.
    let mut hunk_start = anchor_pos;
    for i in (0..anchor_line_idx).rev() {
      let hunk_line = &lines[i];
      if matches!(hunk_line.kind, LineKind::Addition) {
        continue;
      }

      if hunk_start == 0 {
        return fail(i);
      }

      let prev_newline = memrchr(b'\n', &source[..hunk_start - 1])
        .map(|p| p + 1)
        .unwrap_or(0);

      let mut line_in_source = &source[prev_newline..hunk_start];
      if let Some(stripped) = line_in_source.strip_suffix(b"\r\n") {
        line_in_source = stripped;
      } else if let Some(stripped) = line_in_source.strip_suffix(b"\n") {
        line_in_source = stripped;
      }

      if line_in_source != hunk_line.text {
        return fail(i);
      }
      hunk_start = prev_newline;
    }

    // Then forwards from the anchor over the rest of the hunk.
    let mut current_source = &source[anchor_pos..];
    for (idx, hunk_line) in lines.iter().enumerate().skip(anchor_line_idx) {
      if matches!(hunk_line.kind, LineKind::Addition) {
        continue;
      }

      let expected = hunk_line.text;
      let Some(after_text) = current_source.strip_prefix(expected) else {
        return fail(idx);
      };

      current_source = match after_text {
        [b'\n', rest @ ..] | [b'\r', b'\n', rest @ ..] => rest,
        [] => after_text,
        _ => return fail(idx),
      };
    }

    Ok((hunk_start, current_source))
  }
}
