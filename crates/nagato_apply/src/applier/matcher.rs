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
      match after_text {
        [b'\n', rest @ ..] | [b'\r', b'\n', rest @ ..] => current_source = rest,
        [] => current_source = after_text,
        _ => {
          return Err(Error::with_line(
            ErrorKind::CouldNotApplyHunk,
            hunk.patch_line_num + 1 + (start_at_hunk_line + i) as u32,
          ));
        }
      };
    }
    Ok(current_source)
  }

  fn get_search_buffer<'s>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk,
  ) -> (&'s [u8], usize) {
    hunk
      .label
      .and_then(|label| {
        let finder = Finder::new(label);
        for pos in finder.find_iter(buffer) {
          if pos == 0 || buffer[pos - 1] == b'\n' {
            let line_end = memchr::memchr(b'\n', &buffer[pos..])
              .map(|i| pos + i + 1)
              .unwrap_or(buffer.len());
            return Some((&buffer[line_end..], line_end));
          }
        }
        None
      })
      .unwrap_or((buffer, 0))
  }

  #[inline]
  pub fn find_match<'s, 'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
    precomputed_finder: Option<&Finder>,
  ) -> Result<(usize, &'s [u8]), Error> {
    let mut it = hunk.lines_to_match();
    let (first_idx, first_line) = match it.next() {
      Some((idx, line)) => (idx, line),
      None => return Ok((0, buffer)),
    };

    let needle = first_line.text;
    let finder_storage;
    let finder = if let Some(f) = precomputed_finder {
      Some(f)
    } else if !needle.is_empty() {
      finder_storage = Some(Finder::new(needle));
      finder_storage.as_ref()
    } else {
      None
    };

    self.search_in_buffer(
      buffer,
      hunk,
      first_idx,
      needle,
      finder,
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

    match (first, second) {
      (None, _) => Ok((0, buffer, None)),
      (Some((i, _)), None) => Ok((0, buffer, Some(i))),
      (Some((first_idx, _)), Some((second_idx, second_line))) => {
        let needle = second_line.text;
        let finder = (!needle.is_empty()).then(|| Finder::new(needle));

        self
          .search_in_buffer(
            buffer,
            hunk,
            second_idx,
            needle,
            finder.as_ref(),
            (buffer, 0),
          )
          .map(|(pos, src)| (pos, src, Some(first_idx)))
      }
    }
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
      // The search buffer is scanned for potential matches using a precomputed finder for a non-empty line of the hunk.
      for match_pos_rel in finder.find_iter(search_buffer) {
        let match_pos = buffer_offset + match_pos_rel;
        // Ensure match starts at a line boundary.
        if match_pos > 0 && buffer[match_pos - 1] != b'\n' {
          continue;
        }

        // If the rest of the buffer is shorter than the minimum expected length, skip.
        if (buffer.len() - match_pos) < needle.len() {
          continue;
        }

        let end_pos = match_pos + needle.len();
        match buffer.get(end_pos..) {
          Some([b'\n', ..]) | Some([b'\r', b'\n', ..]) | Some([]) | None => {}
          _ => continue,
        };

        if let Some(hunk_start) =
          self.find_hunk_start(buffer, match_pos, lines_before)
        {
          match self.verify_match(&buffer[hunk_start..], hunk, 0) {
            Ok(final_source) => return Ok((hunk_start, final_source)),
            Err(e) => {
              let offset =
                e.line.unwrap_or(0).saturating_sub(hunk.patch_line_num);
              if offset > max_offset || best_error.is_none() {
                max_offset = offset;
                best_error = Some(e);
              }
            }
          }
        }
      }
    } else {
      let mut match_pos = buffer_offset;
      while match_pos <= buffer.len() {
        let remaining = &buffer[match_pos..];
        let (line, rest) = match nagato_core::get_line(remaining) {
          Some(res) => res,
          None => break,
        };
        let consumed = remaining.len() - rest.len();

        if line.is_empty() {
          if let Some(hunk_start) =
            self.find_hunk_start(buffer, match_pos, lines_before)
          {
            match self.verify_match(&buffer[hunk_start..], hunk, 0) {
              Ok(final_source) => return Ok((hunk_start, final_source)),
              Err(e) => {
                let offset =
                  e.line.unwrap_or(0).saturating_sub(hunk.patch_line_num);
                if offset > max_offset || best_error.is_none() {
                  max_offset = offset;
                  best_error = Some(e);
                }
              }
            }
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

    // If we only need one line before, it's a simple memrchr.
    if lines_before == 1 {
      return match memchr::memrchr(b'\n', &buffer[..current_pos - 1]) {
        Some(idx) => Some(idx + 1),
        None => Some(0),
      };
    }

    // Find the nth newline backwards.
    let mut search_end = current_pos - 1;
    while count < lines_before {
      match memchr::memrchr(b'\n', &buffer[..search_end]) {
        Some(idx) => {
          current_pos = idx + 1;
          if idx == 0 {
            count += 1;
            break;
          }
          search_end = idx;
          count += 1;
        }
        None => {
          if count + 1 == lines_before {
            return Some(0);
          }
          return None;
        }
      }
    }

    if count == lines_before {
      Some(current_pos)
    } else {
      None
    }
  }
}
