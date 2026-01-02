use std::io::Write;

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
    } else {
      self.first_line = false;
    }
    self.output.write_all(line).map_err(Into::into)
  }

  /// Write a block of data, splitting it into lines and updating the source line counter.
  fn write_block(&mut self, block: &[u8]) -> Result<(), Error> {
    for line in block.lines() {
      self.write_line(line)?;
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

  /// Extract the search buffer based on hunk labels if present.
  fn get_search_buffer(
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
  fn search_match<'p>(
    &self,
    buffer: &'s [u8],
    hunk: &Hunk<'p>,
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
    first_line_to_match: &Line,
  ) -> Result<(usize, &'s [u8]), Error> {
    let needle = first_line_to_match.text;
    let finder = Finder::new(needle);
    let (search_buffer, buffer_offset) = self.get_search_buffer(buffer, hunk);

    let mut best_error = None;
    let mut max_offset = 0;

    for match_pos_rel in finder.find_iter(search_buffer) {
      let match_pos = buffer_offset + match_pos_rel;
      if match_pos > 0 && buffer[match_pos - 1] != b'\n' {
        continue;
      }

      let end_pos = match_pos + needle.len();
      let next_source = match &buffer[end_pos..] {
        // Handle all common line ending variations and EOF.
        [b'\n', rest @ ..] | [b'\r', b'\n', rest @ ..] | [b'\r', rest @ ..] => {
          rest
        }
        [] => &[],
        // If the match isn't followed by a newline or EOF, it's a partial line match and should be skipped.
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

  /// Find and apply a single hunk to the source.
  #[inline]
  pub fn find_and_apply_hunk<'p>(
    &mut self,
    hunk: &Hunk<'p>,
  ) -> Result<(), Error> {
    let lines_to_match = hunk
      .lines
      .iter()
      .enumerate()
      .filter(|(_, l)| !matches!(l.kind, LineKind::Addition));

    let source = self.source_at();

    // Attempt match
    let mut iter = lines_to_match.clone();
    let (match_pos, final_source, skipped_line_index) = match iter.next() {
      Some((_, first)) => match self.search_match(source, hunk, iter, first) {
        Ok((pos, src)) => (pos, src, None),
        Err(e) if !hunk.has_header => {
          let mut alt_iter = lines_to_match.clone();
          let first_item = alt_iter.next();
          match alt_iter.next() {
            Some((_, second)) => self
              .search_match(source, hunk, alt_iter, second)
              .map(|(pos, src)| (pos, src, first_item.map(|(i, _)| i)))
              .map_err(|_| e)?,
            None => (0, source, first_item.map(|(i, _)| i)),
          }
        }
        Err(e) => return Err(e),
      },
      None => (0, source, None),
    };

    let skipped = &source[..match_pos];
    self.write_block(skipped)?;
    self.pos += source.len() - final_source.len();
    self.current_source_line += hunk.old_span;

    for (i, line) in hunk.lines.iter().enumerate() {
      if Some(i) != skipped_line_index {
        match line.kind {
          LineKind::Addition | LineKind::Context => {
            self.write_line(line.text)?
          }
          LineKind::Deletion => {}
        }
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

    self.find_and_apply_hunk(hunk)
  }

  /// Verify that the source matches the expected hash for a binary patch.
  pub fn verify_binary_source(&self, patch: &Patch<'_>) -> Result<(), Error> {
    if let Some(old_hash_bytes) = patch.old_hash {
      if old_hash_bytes.len() >= 7 && !old_hash_bytes.iter().all(|&b| b == b'0')
      {
        let source = self.source_at();
        let mut hasher = Sha1::new();
        write!(hasher, "blob {}\0", source.len()).unwrap();
        hasher.update(source);
        let result = hasher.finalize();
        let mut hex_hash = [0u8; 40];
        hex::encode_to_slice(result, &mut hex_hash).unwrap();

        if !hex_hash.starts_with(old_hash_bytes) {
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
            Err(e) if e.kind == ErrorKind::BinaryPatchSourceMismatch => {
              continue
            }
            Err(e) => return Err(e),
          }
        }
      }
    }

    Err(Error::new(ErrorKind::CouldNotApplyHunk))
  }

  /// Process the entire patch.
  pub fn process(mut self, patch: &Patch<'_>) -> Result<(), Error> {
    if !patch.binary_fragments.is_empty() {
      return self.process_binary(patch);
    }

    // Verify source hash for text patches if index header is present.
    // This is a strict check to prevent applying patches to the wrong file version.
    self.verify_binary_source(patch)?;

    if !patch.hunks.is_empty() && !patch.hunks[0].has_header {
      // Hunkless patches (no @@ headers) may be out of order.
      // We sort them by their best match position in the source to ensure sequential application.
      let mut hunks_with_pos = Vec::with_capacity(patch.hunks.len());
      for hunk in patch.hunks.iter() {
        let mut lines = hunk
          .lines
          .iter()
          .enumerate()
          .filter(|(_, l)| !matches!(l.kind, LineKind::Addition));

        let (pos, _) = match lines.next() {
          Some((_, first)) => {
            self.search_match(self.source_at(), hunk, lines, first)?
          }
          None => (0, self.source_at()),
        };
        hunks_with_pos.push((pos, hunk));
      }
      hunks_with_pos.sort_by_key(|(pos, _)| *pos);
      for (_, hunk) in hunks_with_pos {
        self.process_hunk(hunk)?;
      }
    } else {
      for hunk in patch.hunks.iter() {
        self.process_hunk(hunk)?;
      }
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
    if !source.is_empty() {
      self.write_block(source)?;
      self.pos = self.source.len();
    }
    Ok(())
  }
}
