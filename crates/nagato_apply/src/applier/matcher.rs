use bstr::ByteSlice;
use memchr::memmem::Finder;
use nagato_core::{Error, ErrorKind};

use crate::{Hunk, Line};

pub struct Matcher;

impl Matcher {
  #[inline]
  pub fn verify_match<'s, 'p>(
    &self,
    source: &'s [u8],
    lines_to_match: &[(usize, &Line<'p>)],
    hunk: &Hunk,
  ) -> Result<&'s [u8], Error> {
    let mut current_source = source;
    for (offset, hunk_line) in lines_to_match {
      let expected = hunk_line.text;
      if !current_source.starts_with(expected) {
        return Err(Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk.patch_line_num + 1 + *offset as u32,
        ));
      }

      let after_text = &current_source[expected.len()..];
      current_source = if after_text.is_empty() {
        after_text
      } else if let Some(rest) = after_text.strip_prefix(b"\n") {
        rest
      } else if let Some(rest) = after_text.strip_prefix(b"\r\n") {
        rest
      } else {
        return Err(Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk.patch_line_num + 1 + *offset as u32,
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
            let line_end = buffer[pos..]
              .find_byte(b'\n')
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
    let first_line = match it.next() {
      Some((_, first)) => first,
      None => return Ok((0, buffer)),
    };

    let needle = first_line.text;
    let finder = if !needle.is_empty() {
      Some(Finder::new(needle))
    } else {
      None
    };

    let remaining_lines: Vec<_> = it.collect();
    self.search_in_buffer(
      buffer,
      hunk,
      &remaining_lines,
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
      (Some((first_idx, _)), Some((_, second_line))) => {
        let needle = second_line.text;
        let finder = if !needle.is_empty() {
          Some(Finder::new(needle))
        } else {
          None
        };

        let remaining_lines: Vec<_> = it.collect();
        self
          .search_in_buffer(
            buffer,
            hunk,
            &remaining_lines,
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
    lines_to_match: &[(usize, &Line<'p>)],
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
        let line_end = remaining.find_byte(b'\n').unwrap_or(remaining.len());
        let line = &remaining[..line_end];
        let line = line.strip_suffix(b"\r").unwrap_or(line);

        if line.is_empty() {
          let next_source = if line_end < remaining.len() {
            if remaining[line_end..].starts_with(b"\r\n") {
              &remaining[line_end + 2..]
            } else {
              &remaining[line_end + 1..]
            }
          } else {
            &[][..]
          };

          match self.verify_match(next_source, lines_to_match, hunk) {
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

        if remaining.is_empty() {
          break;
        }
        match_pos += line_end + 1;
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

      let remaining = &buffer[match_pos..];
      let line_end = remaining.find_byte(b'\n').unwrap_or(remaining.len());
      let line = &remaining[..line_end];
      let line = line.strip_suffix(b"\r").unwrap_or(line);

      if line != needle {
        continue;
      }

      let next_source = if line_end < remaining.len() {
        &remaining[line_end + 1..]
      } else {
        &[][..]
      };

      match self.verify_match(next_source, lines_to_match, hunk) {
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
