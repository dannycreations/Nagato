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
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)>,
    hunk: &Hunk,
  ) -> Result<&'s [u8], Error> {
    let mut current_source = source;
    for (offset, hunk_line) in lines_to_match {
      let expected = hunk_line.text;
      let len = expected.len();

      if current_source.len() < len || &current_source[..len] != expected {
        return Err(Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk.patch_line_num + 1 + offset as u32,
        ));
      }

      let after_text = &current_source[len..];
      if after_text.starts_with(b"\r\n") {
        current_source = &after_text[2..];
      } else if after_text.starts_with(b"\n") {
        current_source = &after_text[1..];
      } else if after_text.is_empty() {
        current_source = after_text;
      } else {
        return Err(Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk.patch_line_num + 1 + offset as u32,
        ));
      }
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
    // Hunk matching starts by identifying the first non-addition line to use as a search needle in the source buffer.
    let mut iter = hunk.lines_to_match();
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

  pub fn find_match_recovery<'s, 'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
  ) -> Result<(usize, &'s [u8], Option<usize>), Error> {
    // Recovery matching attempts to find a hunk by skipping the first expected line when an exact match fails.
    let mut iter = hunk.lines_to_match();
    let first_item = iter.next();

    match iter.next() {
      Some((_, second)) => self
        .search_in_buffer(buffer, hunk, iter, second.text, (buffer, 0))
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
    let mut best_error = None;
    let mut max_offset = 0;

    if needle.is_empty() {
      let mut match_pos = buffer_offset;
      while match_pos <= buffer.len() {
        let remaining = &buffer[match_pos..];
        let (line, next_source) = match remaining.split_once_str(b"\n") {
          Some((l, r)) => (l.strip_suffix(b"\r").unwrap_or(l), r),
          None => (remaining, &[][..]),
        };

        if line.is_empty() {
          match self.verify_match(next_source, lines_to_match.clone(), hunk) {
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
        match_pos += line.len()
          + if remaining[line.len()..].starts_with(b"\r\n") {
            2
          } else {
            1
          };
      }

      return Err(best_error.unwrap_or_else(|| {
        let error_line =
          hunk.patch_line_num + if hunk.has_header { 0 } else { 1 };
        Error::with_line(ErrorKind::CouldNotApplyHunk, error_line)
      }));
    }

    let finder = Finder::new(needle);

    // The search buffer is scanned for potential matches using a precomputed finder for the first line of the hunk.
    for match_pos_rel in finder.find_iter(search_buffer) {
      let match_pos = buffer_offset + match_pos_rel;
      // Ensure match starts at a line boundary.
      if match_pos > 0 && buffer[match_pos - 1] != b'\n' {
        continue;
      }

      let remaining = &buffer[match_pos..];
      let (line, next_source) = match remaining.split_once_str(b"\n") {
        Some((l, r)) => (l.strip_suffix(b"\r").unwrap_or(l), r),
        None => (remaining, &[][..]),
      };

      if line != needle {
        continue;
      }

      match self.verify_match(next_source, lines_to_match.clone(), hunk) {
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
