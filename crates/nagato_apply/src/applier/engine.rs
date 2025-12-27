use std::{io::Write, str};

use bstr::ByteSlice;
use memchr::{memchr, memchr_iter, memmem};
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
  fn write_block(&mut self, block: &[u8], lines: u32) -> Result<(), Error> {
    if block.is_empty() {
      return Ok(());
    }

    // Efficiently write blocks by avoiding line-by-line iteration when possible.
    // We strip the trailing newline because write_line/write_block logic
    // prepends newlines for subsequent content.
    if !self.first_line {
      self.output.write_all(b"\n")?;
    }

    // If block has CRs, we must normalize to \n for our internal processing
    // which assumes \n as the separator and prepends it.
    if memchr(b'\r', block).is_some() {
      let mut lines_iter = block.lines();
      if let Some(first) = lines_iter.next() {
        self.output.write_all(first)?;
        for line in lines_iter {
          self.output.write_all(b"\n")?;
          self.output.write_all(line)?;
        }
      }
    } else {
      self
        .output
        .write_all(block.strip_suffix(b"\n").unwrap_or(block))?;
    }

    self.first_line = false;
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
    let mut lines_to_skip =
      target_line.saturating_sub(self.current_source_line);

    if lines_to_skip > 0 {
      let mut count = 0;
      let mut end_offset = 0;
      for pos in memchr_iter(b'\n', self.source) {
        count += 1;
        end_offset = pos + 1;
        if count == lines_to_skip {
          break;
        }
      }

      if count > 0 {
        let block = &self.source[..end_offset];
        self.write_block(block, count)?;
        self.source = &self.source[end_offset..];
        lines_to_skip -= count;
      }
    }

    // Fallback or handle remaining lines
    for _ in 0..lines_to_skip {
      if let Some(line) = self.consume_line() {
        self.write_line(line)?;
        self.current_source_line += 1;
      } else if hunk.old_span > 0 {
        return Err(Error::with_line(
          ErrorKind::CouldNotApplyHunk,
          hunk.patch_line_num,
        ));
      } else {
        break;
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
    let needle = first_line_to_match.text;

    if needle.is_empty() {
      while let Some((line, next_source)) = get_line(self.source) {
        if line == needle {
          match self.verify_match(next_source, lines_to_match.clone(), hunk) {
            Ok(final_source) => {
              self.source = final_source;
              self.current_source_line += 1;
              return Ok(());
            }
            Err(e) => return Err(e),
          }
        }

        self.write_line(line)?;
        self.source = next_source;
        self.current_source_line += 1;
      }
      return Err(Error::with_line(
        ErrorKind::CouldNotApplyHunk,
        hunk.patch_line_num,
      ));
    }

    let finder = Finder::new(needle);
    for match_pos in finder.find_iter(self.source) {
      // Ensure match is at the start of a line and ends at a line boundary.
      if match_pos > 0 && self.source[match_pos - 1] != b'\n' {
        continue;
      }

      let end_pos = match_pos + needle.len();
      let next_source = if end_pos == self.source.len() {
        &self.source[end_pos..]
      } else if self.source[end_pos] == b'\n' {
        &self.source[end_pos + 1..]
      } else if self.source[end_pos] == b'\r'
        && self.source.get(end_pos + 1) == Some(&b'\n')
      {
        &self.source[end_pos + 2..]
      } else {
        continue;
      };

      match self.verify_match(next_source, lines_to_match.clone(), hunk) {
        Ok(final_source) => {
          let skipped = &self.source[..match_pos];
          let lines_skipped = memchr_iter(b'\n', skipped).count() as u32;
          self.write_block(skipped, lines_skipped)?;

          self.source = final_source;
          self.current_source_line += 1;
          return Ok(());
        }
        Err(e) => {
          // If this was the expected position, return the error immediately.
          // Otherwise, continue searching.
          let target_line = hunk.old_line.saturating_sub(1);
          if self.current_source_line == target_line {
            return Err(e);
          }
        }
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
        hasher.update(b"blob ");
        hasher.update(self.source.len().to_string().as_bytes());
        hasher.update(b"\0");
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

    let lines = memchr_iter(b'\n', source).count() as u32;
    self.write_block(source, lines)?;
    self.source = &[];
    Ok(())
  }
}
