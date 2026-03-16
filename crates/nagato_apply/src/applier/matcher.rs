use memchr::memmem::Finder;
use nagato_core::{Error, ErrorKind};

use crate::Hunk;

pub struct Matcher;

impl Matcher {
  #[inline]
  pub fn verify_match<'s, 'p>(
    &self,
    source: &'s [u8],
    hunk: &Hunk<'p>,
    start_at_hunk_line: usize,
  ) -> Result<&'s [u8], Error> {
    let mut current_source = source;
    for (i, hunk_line) in hunk.lines[start_at_hunk_line..].iter().enumerate() {
      if matches!(hunk_line.kind, crate::LineKind::Addition) {
        continue;
      }

      let expected = hunk_line.text;
      if current_source.len() < expected.len()
        || !current_source[..expected.len()].eq(expected)
      {
        return Err(Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk.patch_line_num + 1 + (start_at_hunk_line + i) as u32,
        ));
      }

      let after_text = &current_source[expected.len()..];
      if let [b'\n', rest @ ..] | [b'\r', b'\n', rest @ ..] = after_text {
        current_source = rest;
        continue;
      }

      if after_text.is_empty() {
        current_source = after_text;
        continue;
      }

      return Err(Error::with_line(
        ErrorKind::CouldNotApplyHunk,
        hunk.patch_line_num + 1 + (start_at_hunk_line + i) as u32,
      ));
    }
    Ok(current_source)
  }

  fn get_search_buffer<'s>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk,
  ) -> (&'s [u8], usize) {
    let Some(label) = hunk.label else {
      return (buffer, 0);
    };

    let finder = Finder::new(label);
    for pos in finder.find_iter(buffer) {
      if pos != 0 && buffer[pos - 1] != b'\n' {
        continue;
      }

      let line_end = match memchr::memchr(b'\n', &buffer[pos..]) {
        Some(i) => pos + i + 1,
        None => buffer.len(),
      };

      return (&buffer[line_end..], line_end);
    }

    (buffer, 0)
  }

  #[inline]
  pub fn find_match<'s, 'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
    precomputed_finder: Option<&Finder>,
  ) -> Result<(usize, &'s [u8]), Error> {
    let mut it = hunk.lines_to_match();
    let Some((first_idx, first_line)) = it.next() else {
      return Ok((0, buffer));
    };

    let needle = first_line.text;
    if let Some(f) = precomputed_finder {
      return self.search_in_buffer(
        buffer,
        hunk,
        first_idx,
        needle,
        Some(f),
        self.get_search_buffer(buffer, hunk),
      );
    }

    let mut finder_storage = None;
    if !needle.is_empty() {
      finder_storage = Some(Finder::new(needle));
    }

    self.search_in_buffer(
      buffer,
      hunk,
      first_idx,
      needle,
      finder_storage.as_ref(),
      self.get_search_buffer(buffer, hunk),
    )
  }

  pub fn find_match_recovery<'s, 'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
  ) -> Result<(usize, &'s [u8], Option<usize>), Error> {
    let mut it = hunk.lines_to_match();
    let first = it.next();
    let second = it.next();

    if first.is_none() {
      return Ok((0, buffer, None));
    }

    let (first_idx, _) = first.unwrap();
    if second.is_none() {
      return Ok((0, buffer, Some(first_idx)));
    }

    let (second_idx, second_line) = second.unwrap();
    let needle = second_line.text;
    let mut finder_storage = None;
    if !needle.is_empty() {
      finder_storage = Some(Finder::new(needle));
    }

    self
      .search_in_buffer(
        buffer,
        hunk,
        second_idx,
        needle,
        finder_storage.as_ref(),
        (buffer, 0),
      )
      .map(|(pos, src)| (pos, src, Some(first_idx)))
  }

  fn search_with_finder<'s, 'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
    lines_before: usize,
    needle: &[u8],
    finder: &Finder,
    (search_buffer, buffer_offset): (&'s [u8], usize),
  ) -> Result<(usize, &'s [u8]), Error> {
    let mut best_error = None;
    let mut max_offset = 0;

    // The search buffer is scanned for potential matches using a precomputed finder for a non-empty line of the hunk.
    for match_pos_rel in finder.find_iter(search_buffer) {
      let match_pos = buffer_offset + match_pos_rel;
      if match_pos > 0 && buffer[match_pos - 1] != b'\n' {
        continue;
      }

      if (buffer.len() - match_pos) < needle.len() {
        continue;
      }

      let end_pos = match_pos + needle.len();
      let after_match = buffer.get(end_pos..);

      let is_line_end = matches!(
        after_match,
        Some([b'\n', ..]) | Some([b'\r', b'\n', ..]) | Some([]) | None
      );

      if !is_line_end {
        continue;
      }

      let hunk_start =
        match self.find_hunk_start(buffer, match_pos, lines_before) {
          Some(s) => s,
          None => continue,
        };

      let match_res = self.verify_match(&buffer[hunk_start..], hunk, 0);
      if let Ok(final_source) = match_res {
        return Ok((hunk_start, final_source));
      }

      let e = match_res.unwrap_err();
      let offset = e.line.unwrap_or(0).saturating_sub(hunk.patch_line_num);
      if offset > max_offset || best_error.is_none() {
        max_offset = offset;
        best_error = Some(e);
      }
    }

    Err(best_error.unwrap_or_else(|| {
      let error_line =
        hunk.patch_line_num + if hunk.has_header { 0 } else { 1 };
      Error::with_line(ErrorKind::CouldNotApplyHunk, error_line)
    }))
  }

  #[inline]
  fn search_in_buffer<'s, 'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
    match_idx: usize,
    needle: &[u8],
    finder: Option<&Finder>,
    (search_buffer, buffer_offset): (&'s [u8], usize),
  ) -> Result<(usize, &'s [u8]), Error> {
    let mut best_error = None;
    let mut max_offset = 0;

    let lines_before = hunk
      .lines_to_match()
      .take_while(|(idx, _)| *idx != match_idx)
      .count();

    if let Some(finder) = finder {
      return self.search_with_finder(
        buffer,
        hunk,
        lines_before,
        needle,
        finder,
        (search_buffer, buffer_offset),
      );
    }

    let mut match_pos = buffer_offset;
    while match_pos <= buffer.len() {
      let remaining = &buffer[match_pos..];
      let Some((line, rest)) = nagato_core::get_line(remaining) else {
        break;
      };
      let consumed = remaining.len() - rest.len();

      if !line.is_empty() {
        if rest.is_empty()
          && !remaining.is_empty()
          && remaining.last() != Some(&b'\n')
        {
          break;
        }
        match_pos += consumed;
        continue;
      }

      let hunk_start = self.find_hunk_start(buffer, match_pos, lines_before);
      if let Some(start) = hunk_start {
        let match_res = self.verify_match(&buffer[start..], hunk, 0);
        if let Ok(final_source) = match_res {
          return Ok((start, final_source));
        }

        let e = match_res.unwrap_err();
        let offset = e.line.unwrap_or(0).saturating_sub(hunk.patch_line_num);
        if offset > max_offset || best_error.is_none() {
          max_offset = offset;
          best_error = Some(e);
        }
      }

      if rest.is_empty()
        && !remaining.is_empty()
        && remaining.last() != Some(&b'\n')
      {
        break;
      }
      match_pos += consumed;
    }

    Err(best_error.unwrap_or_else(|| {
      let error_line =
        hunk.patch_line_num + if hunk.has_header { 0 } else { 1 };
      Error::with_line(ErrorKind::CouldNotApplyHunk, error_line)
    }))
  }

  fn find_hunk_start(
    &self,
    buffer: &[u8],
    match_pos: usize,
    lines_before: usize,
  ) -> Option<usize> {
    if lines_before == 0 {
      return Some(match_pos);
    }
    if match_pos == 0 {
      return None;
    }

    let mut current_pos = match_pos;
    let mut count = 0;

    while count < lines_before {
      if current_pos == 0 {
        return None;
      }
      let Some(idx) = memchr::memrchr(b'\n', &buffer[..current_pos - 1]) else {
        if count + 1 == lines_before {
          return Some(0);
        }
        return None;
      };

      current_pos = idx + 1;
      count += 1;
    }

    Some(current_pos)
  }
}
