use std::{io::Write, str};

use bstr::ByteSlice;
use memchr::memmem::Finder;
use nagato_core::{get_line, Error, ErrorKind};
use sha1::{Digest, Sha1};

use crate::{binary, BinaryKind, Hunk, Line, LineKind, Patch};

/// The Applier engine responsible for applying patches to byte slices.
pub struct Applier<'s, 'b, W: Write + ?Sized> {
  pub output: &'b mut W,
  /// The full original source. We use slices to track progress.
  pub source: &'s [u8],
  /// The current byte offset in the full source.
  pub pos: usize,
  pub first_line: bool,
  pub current_source_line: u32,
}

impl<'s, 'b, W: Write + ?Sized> Applier<'s, 'b, W> {
  pub fn new(output: &'b mut W, source: &'s [u8]) -> Self {
    Self {
      output,
      source,
      pos: 0,
      first_line: true,
      current_source_line: 0,
    }
  }

  /// Returns the remaining source content.
  #[inline(always)]
  fn source_at(&self) -> &'s [u8] {
    &self.source[self.pos..]
  }

  /// Write a line to the output, handling line endings.
  /// We prepend a newline for every line except the first to ensure correct formatting.
  #[inline]
  pub fn write_line(&mut self, line: &[u8]) -> Result<(), Error> {
    if !self.first_line {
      self.output.write_all(b"\n")?;
    }
    self.first_line = false;
    self.output.write_all(line)?;
    Ok(())
  }

  /// Write a block of data, splitting it into lines and prepending newlines as needed.
  /// This ensures consistent line endings and updates the source line counter.
  fn write_block(&mut self, block: &[u8]) -> Result<(), Error> {
    for line in block.lines() {
      self.write_line(line)?;
      self.current_source_line += 1;
    }
    Ok(())
  }

  /// Advance the source and output to the start of a hunk.
  pub fn advance_to_hunk(&mut self, hunk: &Hunk) -> Result<(), Error> {
    let target_line = hunk.old_line.saturating_sub(1);
    let lines_to_skip = target_line.saturating_sub(self.current_source_line);

    for _ in 0..lines_to_skip {
      let source = self.source_at();
      let (line, next_source) = get_line(source).ok_or_else(|| {
        Error::with_line(ErrorKind::CouldNotApplyHunk, hunk.patch_line_num + 1)
      })?;
      self.write_line(line)?;
      self.pos += source.len() - next_source.len();
      self.current_source_line += 1;
    }
    Ok(())
  }

  /// Verify if the source matches the expected hunk lines.
  #[inline]
  pub fn verify_match<'p>(
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

  /// Find a match for a hunk in the source, allowing for fuzzing/offset.
  pub fn find_hunk_match<'p>(
    &mut self,
    hunk: &Hunk<'p>,
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
    first_line_to_match: &Line,
  ) -> Result<(), Error> {
    let source = self.source_at();
    let (match_pos, final_source) =
      self.search_match(source, hunk, lines_to_match, first_line_to_match)?;

    let skipped = &source[..match_pos];
    self.write_block(skipped)?;
    self.pos += source.len() - final_source.len();
    self.current_source_line += 1;
    Ok(())
  }

  fn search_match<'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
    first_line_to_match: &Line,
  ) -> Result<(usize, &'s [u8]), Error> {
    let needle = first_line_to_match.text;
    let finder = Finder::new(needle);
    let mut best_error = None;
    let mut max_offset = 0;

    for match_pos in finder.find_iter(buffer) {
      // Ensure match is at the start of a line.
      if match_pos > 0 && buffer[match_pos - 1] != b'\n' {
        continue;
      }

      // Ensure match ends at a line boundary.
      let end_pos = match_pos + needle.len();
      let remaining = &buffer[end_pos..];
      let next_source = if remaining.is_empty() {
        remaining
      } else if let Some(rest) = remaining.strip_prefix(b"\n") {
        rest
      } else if let Some(rest) = remaining.strip_prefix(b"\r\n") {
        rest
      } else if remaining.starts_with(b"\r") {
        // Handle cases where we might have a \r without \n, or just skip it
        &remaining[1..]
      } else {
        continue;
      };

      match self.verify_match(next_source, lines_to_match.clone(), hunk) {
        Ok(final_source) => return Ok((match_pos, final_source)),
        Err(e) => {
          // If we have a specific line expectation, return the error immediately.
          if hunk.old_line > 0
            && self.current_source_line == hunk.old_line.saturating_sub(1)
          {
            return Err(e);
          }

          // Otherwise, track the "best" match (the one that went furthest).
          let offset = e.line.unwrap_or(0).saturating_sub(hunk.patch_line_num);
          if offset >= max_offset {
            max_offset = offset;
            best_error = Some(e);
          }
        }
      }
    }

    Err(best_error.unwrap_or_else(|| {
      let error_line = if hunk.has_header {
        hunk.patch_line_num
      } else {
        hunk.patch_line_num + 1
      };
      Error::with_line(ErrorKind::CouldNotApplyHunk, error_line)
    }))
  }

  /// Find and apply a single hunk to the source.
  pub fn find_and_apply_hunk<'p>(
    &mut self,
    hunk: &Hunk<'p>,
  ) -> Result<(), Error> {
    let mut lines_to_match = hunk
      .lines
      .iter()
      .enumerate()
      .filter(|(_, l)| !matches!(l.kind, LineKind::Addition));
    let first_line_to_match = if let Some((_, line)) = lines_to_match.next() {
      line
    } else {
      return Ok(());
    };

    self.find_hunk_match(hunk, lines_to_match, first_line_to_match)?;
    self.current_source_line += hunk.old_span.saturating_sub(1);

    for line in &hunk.lines {
      match line.kind {
        LineKind::Addition | LineKind::Context => self.write_line(line.text)?,
        LineKind::Deletion => {}
      }
    }

    Ok(())
  }

  /// Process a single hunk by advancing to it and applying changes.
  pub fn process_hunk<'p>(&mut self, hunk: &Hunk<'p>) -> Result<(), Error> {
    if hunk.old_span == 0 {
      hunk
        .lines
        .iter()
        .filter(|l| matches!(l.kind, LineKind::Addition))
        .try_for_each(|l| self.write_line(l.text))?;
      return Ok(());
    }

    if hunk.old_line > 0 {
      self.advance_to_hunk(hunk)?;
    }
    self.find_and_apply_hunk(hunk)
  }

  /// Verify that the source matches the expected hash for a binary patch.
  pub fn verify_binary_source(&self, patch: &Patch<'_>) -> Result<(), Error> {
    if let Some(old_hash_bytes) = patch.old_hash {
      if old_hash_bytes.len() >= 7 {
        let source = self.source_at();
        let mut hasher = Sha1::new();
        write!(hasher, "blob {}\0", source.len()).unwrap();
        hasher.update(source);
        let result = hasher.finalize();
        let hex_hash = hex::encode(result);

        let old_hash_str = str::from_utf8(old_hash_bytes)
          .map_err(|_| Error::new(ErrorKind::InvalidIndexLine))?;

        if !hex_hash.starts_with(old_hash_str)
          && old_hash_str.chars().any(|c| c != '0')
        {
          return Err(Error::new(ErrorKind::BinaryPatchSourceMismatch));
        }
      }
    }
    Ok(())
  }

  /// Process a binary patch.
  pub fn process_binary(&mut self, patch: &Patch<'_>) -> Result<(), Error> {
    self.verify_binary_source(patch)?;

    for fragment in &patch.binary_fragments {
      match fragment.kind {
        BinaryKind::Literal => {
          return binary::decode_base85(&fragment.data, &mut self.output);
        }
        BinaryKind::Delta => {
          let mut decoded = binary::new_base85_decoder(&fragment.data);
          match binary::apply_delta(
            &mut decoded,
            self.source_at(),
            &mut self.output,
          ) {
            Ok(_) => return Ok(()),
            Err(Error {
              kind: ErrorKind::BinaryPatchSourceMismatch,
              ..
            }) => continue,
            Err(e) => return Err(e),
          }
        }
      }
    }

    Err(ErrorKind::CouldNotApplyHunk.into())
  }

  /// Process the entire patch.
  pub fn process(mut self, patch: &Patch<'_>) -> Result<(), Error> {
    if !patch.binary_fragments.is_empty() {
      return self.process_binary(patch);
    }

    let mut hunks = patch.hunks.clone();

    // If we have headerless hunks, they might be out of order.
    // We sort them by their match position in the original source.
    if !hunks.is_empty() && !hunks[0].has_header {
      let mut hunks_with_pos = Vec::with_capacity(hunks.len());
      for hunk in &hunks {
        let mut lines_to_match = hunk
          .lines
          .iter()
          .enumerate()
          .filter(|(_, l)| !matches!(l.kind, LineKind::Addition));
        let first_line_to_match = if let Some((_, line)) = lines_to_match.next()
        {
          line
        } else {
          hunks_with_pos.push((0, hunk.clone()));
          continue;
        };

        match self.search_match(
          self.source,
          hunk,
          lines_to_match,
          first_line_to_match,
        ) {
          Ok((pos, _)) => hunks_with_pos.push((pos, hunk.clone())),
          Err(e) => return Err(e),
        }
      }
      hunks_with_pos.sort_by_key(|(pos, _)| *pos);
      hunks = hunks_with_pos.into_iter().map(|(_, h)| h).collect();
    }

    for hunk in &hunks {
      self.process_hunk(hunk)?;
    }

    // Write any remaining source content to the output.
    self.flush_remaining_source()?;

    // Ensure final newline unless suppressed by patch metadata.
    if !patch.new_file_no_newline && !patch.binary && !self.first_line {
      self.output.write_all(b"\n")?;
    }
    Ok(())
  }

  /// Write remaining source lines to the output.
  fn flush_remaining_source(&mut self) -> Result<(), Error> {
    let source = self.source_at();
    if source.is_empty() {
      return Ok(());
    }

    self.write_block(source)?;
    self.pos = self.source.len();
    Ok(())
  }
}
