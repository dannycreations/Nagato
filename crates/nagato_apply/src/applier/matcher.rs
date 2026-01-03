use bstr::ByteSlice;
use memchr::memmem::Finder;
use nagato_core::{get_line, Error, ErrorKind};

use crate::{Hunk, Line, LineKind};

/// Hunk matching engine.
///
/// Implements a line-based search algorithm with support for:
/// 1. Exact matching (default).
/// 2. Label-based search optimization (using `label` metadata).
/// 3. Recovery matching (fuzzy search when exact match fails).
pub struct Matcher;

impl Matcher {
  /// Verify if the source matches the expected hunk lines.
  #[inline]
  pub fn verify_match<'s, 'p>(
    &self,
    mut source: &'s [u8],
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)>,
    hunk: &Hunk,
  ) -> Result<&'s [u8], Error> {
    for (offset, hunk_line) in lines_to_match {
      let (line, next_source) = get_line(source).ok_or_else(|| {
        Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk.patch_line_num + 1 + offset as u32,
        )
      })?;

      if line != hunk_line.text {
        return Err(Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk.patch_line_num + 1 + offset as u32,
        ));
      }
      source = next_source;
    }
    Ok(source)
  }

  /// Extract the search buffer based on hunk labels if present.
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
    let lines_to_match = hunk
      .lines
      .iter()
      .enumerate()
      .filter(|(_, l)| !matches!(l.kind, LineKind::Addition));

    let mut iter = lines_to_match.clone();
    let first_line_to_match = match iter.next() {
      Some((_, first)) => first,
      None => return Ok((0, buffer)),
    };

    self.search_in_buffer(
      buffer,
      hunk,
      iter,
      first_line_to_match.text,
      self.get_search_buffer(buffer, hunk),
    )
  }

  /// Attempt to find a match by skipping the first line of the hunk.
  /// This provides basic recovery when the context surrounding a change has shifted.
  pub fn find_match_recovery<'s, 'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
  ) -> Result<(usize, &'s [u8], Option<usize>), Error> {
    let lines_to_match = hunk
      .lines
      .iter()
      .enumerate()
      .filter(|(_, l)| !matches!(l.kind, LineKind::Addition));

    let mut alt_iter = lines_to_match.clone();
    let first_item = alt_iter.next();

    match alt_iter.next() {
      Some((_, second)) => self
        .search_in_buffer(buffer, hunk, alt_iter, second.text, (buffer, 0))
        .map(|(pos, src)| (pos, src, first_item.map(|(i, _)| i))),
      None => Ok((0, buffer, first_item.map(|(i, _)| i))),
    }
  }

  #[inline]
  fn search_in_buffer<'s, 'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
    needle: &[u8],
    (search_buffer, buffer_offset): (&'s [u8], usize),
  ) -> Result<(usize, &'s [u8]), Error> {
    let finder = Finder::new(needle);
    let mut best_error = None;
    let mut max_offset = 0;

    for match_pos_rel in finder.find_iter(search_buffer) {
      let match_pos = buffer_offset + match_pos_rel;
      // Ensure match starts at a line boundary.
      if match_pos > 0 && buffer[match_pos - 1] != b'\n' {
        continue;
      }

      let end_pos = match_pos + needle.len();
      let next_source = match buffer.get(end_pos..) {
        Some([b'\n', rest @ ..])
        | Some([b'\r', b'\n', rest @ ..])
        | Some([b'\r', rest @ ..]) => rest,
        Some([]) => &[],
        _ => continue,
      };

      match self.verify_match(next_source, lines_to_match.clone(), hunk) {
        Ok(final_source) => return Ok((match_pos, final_source)),
        Err(e) => {
          let offset = e.line.unwrap_or(0).saturating_sub(hunk.patch_line_num);
          if offset >= max_offset {
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
