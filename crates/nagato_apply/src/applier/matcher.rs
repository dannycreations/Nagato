use bstr::ByteSlice;
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
      if !current_source.starts_with(expected) {
        return Err(Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk.patch_line_num + 1 + (start_at_hunk_line + i) as u32,
        ));
      }

      let after_text = &current_source[expected.len()..];
      current_source = if let Some(rest) = after_text.strip_prefix(b"\n") {
        rest
      } else if let Some(rest) = after_text.strip_prefix(b"\r\n") {
        rest
      } else if after_text.is_empty() {
        after_text
      } else {
        return Err(Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk.patch_line_num + 1 + (start_at_hunk_line + i) as u32,
        ));
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
        buffer
          .find(label)
          .filter(|&pos| pos == 0 || buffer[pos - 1] == b'\n')
          .map(|pos| {
            let line_end = memchr::memchr(b'\n', &buffer[pos..])
              .map(|i| pos + i + 1)
              .unwrap_or(buffer.len());
            (&buffer[line_end..], line_end)
          })
      })
      .unwrap_or((buffer, 0))
  }

  #[inline]
  pub fn find_match<'s, 'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
  ) -> Result<(usize, &'s [u8]), Error> {
    let mut it = hunk.lines_to_match();
    let (first_idx, first_line) = match it.next() {
      Some((idx, line)) => (idx, line),
      None => return Ok((0, buffer)),
    };

    let needle = first_line.text;
    let finder = if !needle.is_empty() {
      Some(Finder::new(needle))
    } else {
      None
    };

    self.search_in_buffer(
      buffer,
      hunk,
      first_idx + 1,
      needle,
      finder.as_ref(),
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
        let finder = if !needle.is_empty() {
          Some(Finder::new(needle))
        } else {
          None
        };

        self
          .search_in_buffer(
            buffer,
            hunk,
            second_idx + 1,
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
    start_at_hunk_line: usize,
    needle: &[u8],
    finder: Option<&Finder>,
    (search_buffer, buffer_offset): (&'s [u8], usize),
  ) -> Result<(usize, &'s [u8]), Error> {
    let mut best_error = None;
    let mut max_offset = 0;

    if let (true, None) = (needle.is_empty(), finder) {
      let mut match_pos = buffer_offset;
      while match_pos <= buffer.len() {
        let remaining = &buffer[match_pos..];
        let (line, rest) = match memchr::memchr(b'\n', remaining) {
          Some(idx) => (&remaining[..idx], &remaining[idx + 1..]),
          None => (remaining, &[][..]),
        };

        let line_stripped = line.strip_suffix(b"\r").unwrap_or(line);

        if line_stripped.is_empty() {
          match self.verify_match(rest, hunk, start_at_hunk_line) {
            Ok(final_source) => return Ok((match_pos, final_source)),
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

        if rest.is_empty()
          && !remaining.is_empty()
          && remaining.last() != Some(&b'\n')
        {
          break;
        }
        match_pos += line.len() + 1;
      }

      return Err(best_error.unwrap_or_else(|| {
        let error_line =
          hunk.patch_line_num + if hunk.has_header { 0 } else { 1 };
        Error::with_line(ErrorKind::CouldNotApplyHunk, error_line)
      }));
    }

    let finder = finder.expect("finder must be present for non-empty needle");

    // The search buffer is scanned for potential matches using a precomputed finder for the first line of the hunk.
    for match_pos_rel in finder.find_iter(search_buffer) {
      let match_pos = buffer_offset + match_pos_rel;
      // Ensure match starts at a line boundary.
      if match_pos > 0 && buffer[match_pos - 1] != b'\n' {
        continue;
      }

      // Fast path: if the rest of the buffer is shorter than the minimum expected length, skip.
      if (buffer.len() - match_pos) < needle.len() {
        continue;
      }

      let end_pos = match_pos + needle.len();
      let next_source = if end_pos < buffer.len() {
        let b = buffer[end_pos];
        if b == b'\n' {
          &buffer[end_pos + 1..]
        } else if b == b'\r' && buffer.get(end_pos + 1) == Some(&b'\n') {
          &buffer[end_pos + 2..]
        } else {
          continue;
        }
      } else {
        &[][..]
      };

      match self.verify_match(next_source, hunk, start_at_hunk_line) {
        Ok(final_source) => return Ok((match_pos, final_source)),
        Err(e) => {
          let offset = e.line.unwrap_or(0).saturating_sub(hunk.patch_line_num);
          if offset > max_offset || best_error.is_none() {
            max_offset = offset;
            best_error = Some(e);
          }
        }
      }
    }

    Err(best_error.unwrap_or_else(|| {
      let error_line =
        hunk.patch_line_num + if hunk.has_header { 0 } else { 1 };
      Error::with_line(ErrorKind::CouldNotApplyHunk, error_line)
    }))
  }
}
