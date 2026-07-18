use memchr::{
  memchr,
  memmem::{self, Finder},
  memrchr,
};
use nagato_core::{get_line, Error, ErrorKind};

use crate::{Hunk, Line, LineKind, Patch};

pub struct Matcher;

impl Matcher {
  #[inline]
  pub fn verify_match<'s, 'p>(
    &self,
    source: &'s [u8],
    lines: &[Line<'p>],
    hunk_patch_line_num: u32,
    anchor_pos: usize,
    anchor_hunk_line_idx: usize,
  ) -> Result<(usize, &'s [u8]), Error> {
    // 1. Verify lines before anchor in reverse
    let mut hunk_start = anchor_pos;
    if anchor_hunk_line_idx > 0 {
      for i in (0..anchor_hunk_line_idx).rev() {
        let hunk_line = &lines[i];
        if matches!(hunk_line.kind, LineKind::Addition) {
          continue;
        }

        // Find the start of the previous line in source
        if hunk_start == 0 {
          return Err(Error::with_line(
            ErrorKind::CouldNotApplyHunk,
            hunk_patch_line_num + 1 + i as u32,
          ));
        }

        let search_limit = hunk_start.saturating_sub(1);
        let prev_newline = memrchr(b'\n', &source[..search_limit])
          .map(|p| p + 1)
          .unwrap_or(0);

        let line_in_source = &source[prev_newline..hunk_start];
        let line_in_source = if line_in_source.ends_with(b"\r\n") {
          &line_in_source[..line_in_source.len() - 2]
        } else if line_in_source.ends_with(b"\n") {
          &line_in_source[..line_in_source.len() - 1]
        } else {
          line_in_source
        };

        if line_in_source != hunk_line.text {
          return Err(Error::with_line(
            ErrorKind::CouldNotApplyHunk,
            hunk_patch_line_num + 1 + i as u32,
          ));
        }
        hunk_start = prev_newline;
      }
    }

    // 2. Verify lines from anchor forward
    let mut current_source = &source[anchor_pos..];
    let forward_len = lines.len() - anchor_hunk_line_idx;
    for i in 0..forward_len {
      let idx = anchor_hunk_line_idx + i;
      let hunk_line = &lines[idx];
      if matches!(hunk_line.kind, LineKind::Addition) {
        continue;
      }

      let expected = hunk_line.text;
      if current_source.len() < expected.len()
        || !current_source[..expected.len()].eq(expected)
      {
        return Err(Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk_patch_line_num + 1 + idx as u32,
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
        hunk_patch_line_num + 1 + idx as u32,
      ));
    }

    Ok((hunk_start, current_source))
  }

  fn get_search_buffer<'s>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk,
  ) -> (&'s [u8], usize) {
    let Some(label) = hunk.label else {
      return (buffer, 0);
    };

    if label.is_empty() {
      return (buffer, 0);
    }

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
  pub fn find_match<'s, 'p>(
    &self,
    buffer: &'s [u8],
    patch: &Patch<'p>,
    hunk: &Hunk<'p>,
    precomputed_finder: Option<&Finder>,
  ) -> Result<(usize, &'s [u8]), Error> {
    let lines = patch.hunk_lines(hunk);

    // Check if the hunk matches at the current position.
    if let Ok((start, remaining)) =
      self.verify_match(buffer, lines, hunk.patch_line_num, 0, 0)
    {
      return Ok((start, remaining));
    }

    let search_hint = self.get_search_buffer(buffer, hunk);

    if let Some((idx, line)) = hunk.best_match_line(lines) {
      if precomputed_finder.is_none() {
        let finder = Finder::new(line.text);
        if let Ok(res) = self.search_in_buffer(
          buffer,
          lines,
          hunk.patch_line_num,
          hunk.has_header,
          idx,
          line.text,
          Some(&finder),
          search_hint,
        ) {
          return Ok(res);
        }
      }
    }

    let (anchor_hunk_line_idx, needle, finder) =
      if let Some(f) = precomputed_finder {
        let (idx, line) = hunk
          .first_non_empty_match_line(lines)
          .expect("precomputed finder requires non-empty line");
        (idx, line.text, Some(f))
      } else if let Some((idx, line)) = hunk.first_non_empty_match_line(lines) {
        (idx, line.text, None)
      } else {
        // Hunk with only empty match lines (gaps/empty context).
        // Find the first match line regardless of content.
        let (idx, line) = match hunk.lines_to_match(lines).next() {
          Some(pair) => pair,
          None => return Ok((0, buffer)),
        };
        return self.search_in_buffer(
          buffer,
          lines,
          hunk.patch_line_num,
          hunk.has_header,
          idx,
          line.text,
          None,
          search_hint,
        );
      };

    let finder_storage;
    let effective_finder = if finder.is_none() {
      finder_storage = Some(Finder::new(needle));
      finder_storage.as_ref()
    } else {
      finder
    };

    self.search_in_buffer(
      buffer,
      lines,
      hunk.patch_line_num,
      hunk.has_header,
      anchor_hunk_line_idx,
      needle,
      effective_finder,
      search_hint,
    )
  }

  #[allow(clippy::too_many_arguments)]
  fn search_with_finder<'s, 'p>(
    &self,
    buffer: &'s [u8],
    lines: &[Line<'p>],
    hunk_patch_line_num: u32,
    hunk_has_header: bool,
    anchor_hunk_line_idx: usize,
    needle: &[u8],
    finder: &Finder,
    (search_buffer, buffer_offset): (&'s [u8], usize),
  ) -> Result<(usize, &'s [u8]), Error> {
    let mut best_error = None;
    let mut max_offset = 0;

    // The search buffer is scanned for potential matches using a precomputed finder for a non-empty line of the hunk.
    for match_pos_rel in finder.find_iter(search_buffer) {
      let anchor_pos = buffer_offset + match_pos_rel;
      if anchor_pos > 0 && buffer[anchor_pos - 1] != b'\n' {
        continue;
      }

      if (buffer.len() - anchor_pos) < needle.len() {
        continue;
      }

      let end_pos = anchor_pos + needle.len();
      let after_match = buffer.get(end_pos..);

      let is_line_end = matches!(
        after_match,
        Some([b'\n', ..]) | Some([b'\r', b'\n', ..]) | Some([]) | None
      );

      if !is_line_end {
        continue;
      }

      let match_res = self.verify_match(
        buffer,
        lines,
        hunk_patch_line_num,
        anchor_pos,
        anchor_hunk_line_idx,
      );
      if let Ok((hunk_start, final_source)) = match_res {
        return Ok((hunk_start, final_source));
      }

      let e = match_res.unwrap_err();
      let offset = e.line.unwrap_or(0).saturating_sub(hunk_patch_line_num);
      if offset > max_offset || best_error.is_none() {
        max_offset = offset;
        best_error = Some(e);
      }
    }

    Err(best_error.unwrap_or_else(|| {
      let error_line =
        hunk_patch_line_num + if hunk_has_header { 0 } else { 1 };
      Error::with_line(ErrorKind::CouldNotApplyHunk, error_line)
    }))
  }

  #[inline]
  #[allow(clippy::too_many_arguments)]
  fn search_in_buffer<'s, 'p>(
    &self,
    buffer: &'s [u8],
    lines: &[Line<'p>],
    hunk_patch_line_num: u32,
    hunk_has_header: bool,
    anchor_hunk_line_idx: usize,
    needle: &[u8],
    finder: Option<&Finder>,
    (search_buffer, buffer_offset): (&'s [u8], usize),
  ) -> Result<(usize, &'s [u8]), Error> {
    let mut best_error = None;
    let mut max_offset = 0;

    if let Some(finder) = finder {
      return self.search_with_finder(
        buffer,
        lines,
        hunk_patch_line_num,
        hunk_has_header,
        anchor_hunk_line_idx,
        needle,
        finder,
        (search_buffer, buffer_offset),
      );
    }

    let mut anchor_pos = buffer_offset;
    while anchor_pos <= buffer.len() {
      let remaining = &buffer[anchor_pos..];
      let Some((line, rest)) = get_line(remaining) else {
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
        anchor_pos += consumed;
        continue;
      }

      let match_res = self.verify_match(
        buffer,
        lines,
        hunk_patch_line_num,
        anchor_pos,
        anchor_hunk_line_idx,
      );
      if let Ok((start, final_source)) = match_res {
        return Ok((start, final_source));
      }

      let e = match_res.unwrap_err();
      let offset = e.line.unwrap_or(0).saturating_sub(hunk_patch_line_num);
      if offset > max_offset || best_error.is_none() {
        max_offset = offset;
        best_error = Some(e);
      }

      if rest.is_empty()
        && !remaining.is_empty()
        && remaining.last() != Some(&b'\n')
      {
        break;
      }
      anchor_pos += consumed;
    }

    Err(best_error.unwrap_or_else(|| {
      let error_line =
        hunk_patch_line_num + if hunk_has_header { 0 } else { 1 };
      Error::with_line(ErrorKind::CouldNotApplyHunk, error_line)
    }))
  }
}
