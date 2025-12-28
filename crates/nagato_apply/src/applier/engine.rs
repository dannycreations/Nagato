use std::{io::Write, str};

use bstr::ByteSlice;
use memchr::{memchr_iter, memmem};
use memmem::Finder;
use nagato_core::{get_line, Error, ErrorKind};
use sha1::{Digest, Sha1};

use crate::{binary, BinaryKind, Hunk, Line, LineKind, Patch};

/// The Applier engine responsible for applying patches to byte slices.
pub struct Applier<'s, 'b, W: Write + ?Sized> {
  pub output: &'b mut W,
  pub source: &'s [u8],
  pub first_line: bool,
  pub current_source_line: u32,
}

impl<'s, 'b, W: Write + ?Sized> Applier<'s, 'b, W> {
  pub fn new(output: &'b mut W, source: &'s [u8]) -> Self {
    Self {
      output,
      source,
      first_line: true,
      current_source_line: 0,
    }
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

  /// Helper to write a block of data that may contain multiple lines.
  /// This centralizes the logic for handling skipped content during hunk matching.
  /// Centralized block writer that handles line endings and updates tracking.
  /// Write a block of data, splitting it into lines and prepending newlines as needed.
  /// This ensures consistent line endings and updates the source line counter.
  fn write_block(&mut self, block: &[u8]) -> Result<(), Error> {
    if block.is_empty() {
      return Ok(());
    }

    let mut lines = 0;
    block.lines().try_for_each(|line| {
      self.write_line(line)?;
      lines += 1;
      Ok::<(), Error>(())
    })?;

    self.current_source_line += lines;
    Ok(())
  }

  /// Consume a single line from the source.
  #[inline]
  pub fn consume_line(&mut self) -> Option<&'s [u8]> {
    let (line, next_source) = get_line(self.source)?;
    self.source = next_source;
    Some(line)
  }

  /// Advance the source and output to the start of a hunk.
  pub fn advance_to_hunk(&mut self, hunk: &Hunk) -> Result<(), Error> {
    let target_line = hunk.old_line.saturating_sub(1);
    let lines_to_skip = target_line.saturating_sub(self.current_source_line);

    if lines_to_skip == 0 {
      return Ok(());
    }

    let end_offset = memchr_iter(b'\n', self.source)
      .nth(lines_to_skip as usize - 1)
      .map(|pos| pos + 1);

    if let Some(offset) = end_offset {
      self.write_block(&self.source[..offset])?;
      self.source = &self.source[offset..];
    } else {
      // Fallback for cases where \n is not found as expected
      for _ in 0..lines_to_skip {
        let line = self.consume_line().ok_or_else(|| {
          if hunk.old_span > 0 {
            Error::with_line(ErrorKind::CouldNotApplyHunk, hunk.patch_line_num)
          } else {
            // This is a bit of an edge case, but if we're skipping lines
            // and hit EOF, and the hunk doesn't expect to match anything (span 0),
            // we just stop skipping.
            ErrorKind::UnexpectedEof.into()
          }
        })?;
        self.write_line(line)?;
        self.current_source_line += 1;
      }
    }
    Ok(())
  }

  /// Verify if the source matches the expected hunk lines.
  #[inline]
  pub fn verify_match<'p>(
    &self,
    mut source: &'s [u8],
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
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
    let (match_pos, final_source) =
      self.search_match(hunk, lines_to_match, first_line_to_match)?;

    let skipped = &self.source[..match_pos];
    self.write_block(skipped)?;
    self.source = final_source;
    self.current_source_line += 1;
    Ok(())
  }

  fn search_match<'p>(
    &self,
    hunk: &Hunk<'p>,
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
    first_line_to_match: &Line,
  ) -> Result<(usize, &'s [u8]), Error> {
    let needle = first_line_to_match.text;

    if needle.is_empty() {
      self.search_match_empty_needle(hunk, lines_to_match)
    } else {
      self.search_match_text_needle(hunk, lines_to_match, needle)
    }
  }

  fn search_match_empty_needle<'p>(
    &self,
    hunk: &Hunk<'p>,
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
  ) -> Result<(usize, &'s [u8]), Error> {
    let mut pos = 0;
    let mut source = self.source;
    while let Some((line, next_source)) = get_line(source) {
      if line.is_empty() {
        if let Ok(final_source) =
          self.verify_match(next_source, lines_to_match.clone(), hunk)
        {
          return Ok((pos, final_source));
        }
      }
      pos += source.len() - next_source.len();
      source = next_source;
    }
    Err(Error::with_line(
      ErrorKind::CouldNotApplyHunk,
      hunk.patch_line_num,
    ))
  }

  fn search_match_text_needle<'p>(
    &self,
    hunk: &Hunk<'p>,
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
    needle: &[u8],
  ) -> Result<(usize, &'s [u8]), Error> {
    let finder = Finder::new(needle);
    for match_pos in finder.find_iter(self.source) {
      // Ensure match is at the start of a line and ends at a line boundary.
      if match_pos > 0 && self.source[match_pos - 1] != b'\n' {
        continue;
      }

      let end_pos = match_pos + needle.len();
      let remaining = &self.source[end_pos..];
      let next_source = match remaining
        .strip_prefix(b"\n")
        .or_else(|| remaining.strip_prefix(b"\r\n"))
      {
        Some(rest) => rest,
        None if remaining.is_empty() => remaining,
        None => continue,
      };

      match self.verify_match(next_source, lines_to_match.clone(), hunk) {
        Ok(final_source) => return Ok((match_pos, final_source)),
        Err(e)
          if self.current_source_line == hunk.old_line.saturating_sub(1) =>
        {
          return Err(e)
        }
        _ => continue,
      }
    }

    Err(Error::with_line(
      ErrorKind::CouldNotApplyHunk,
      hunk.patch_line_num,
    ))
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
        let mut hasher = Sha1::new();
        write!(hasher, "blob {}\0", self.source.len()).unwrap();
        hasher.update(self.source);
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
          match binary::apply_delta(&mut decoded, self.source, &mut self.output)
          {
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

    for hunk in &patch.hunks {
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
    let source = self.source;
    if source.is_empty() {
      return Ok(());
    }

    self.write_block(source)?;
    self.source = &[];
    Ok(())
  }
}
