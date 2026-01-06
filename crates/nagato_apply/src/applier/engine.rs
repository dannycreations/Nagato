use std::io::Write;

use bstr::ByteSlice;
use nagato_core::{Error, ErrorKind, LineWriter};
use sha1::{Digest, Sha1};

use crate::{binary, BinaryKind, Hunk, LineKind, Matcher, Patch};

/// The Applier engine responsible for applying patches to byte slices.
pub struct Applier<'s, 'b, W: Write + ?Sized> {
  writer: LineWriter<'b, W>,
  /// The full original source. We use slices to track progress.
  pub source: &'s [u8],
  /// The current byte offset in the full source.
  pub pos: usize,
  pub current_source_line: u32,
}

impl<'s, 'b, W: Write + ?Sized> Applier<'s, 'b, W> {
  pub fn new(output: &'b mut W, source: &'s [u8]) -> Self {
    Self {
      writer: LineWriter::new(output),
      source,
      pos: 0,
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
    self.writer.write_line(line).map_err(Into::into)
  }

  /// Write a block of data, splitting it into lines and updating the source line counter.
  /// Uses bstr's line iterator for efficient byte-level line splitting.
  fn write_block(&mut self, block: &[u8]) -> Result<(), Error> {
    block.lines().try_for_each(|l| {
      self.write_line(l)?;
      self.current_source_line += 1;
      Ok(())
    })
  }

  /// Find and apply a single hunk to the source.
  #[inline]
  pub fn find_and_apply_hunk<'p>(
    &mut self,
    hunk: &Hunk<'p>,
  ) -> Result<(), Error> {
    let source = self.source_at();
    let matcher = Matcher;

    // Attempt match
    let (match_pos, final_source, skipped_line_index) = matcher
      .find_match(source, hunk)
      .map(|(pos, src)| (pos, src, None))
      .or_else(|e| {
        if !hunk.has_header {
          matcher.find_match_recovery(source, hunk)
        } else {
          Err(e)
        }
      })?;

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
        let _ = write!(hasher, "blob {}\0", source.len());
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
    for fragment in &patch.binary_fragments {
      match fragment.kind {
        BinaryKind::Literal => {
          return binary::decode_base85(&fragment.data, self.writer.output());
        }
        BinaryKind::Delta => {
          let mut decoded = binary::new_base85_decoder(&fragment.data);
          match binary::apply_delta(
            &mut decoded,
            self.source_at(),
            self.writer.output(),
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
    // Verify source hash if index header is present.
    self.verify_binary_source(patch)?;

    if !patch.binary_fragments.is_empty() {
      return self.process_binary(patch);
    }

    if !patch.hunks.is_empty() {
      if !patch.hunks[0].has_header {
        self.process_hunkless_patches(patch)?;
      } else {
        for hunk in patch.hunks.iter() {
          self.process_hunk(hunk)?;
        }
      }
    }

    // Write any remaining source content to the output.
    self.flush_remaining_source()?;

    // Ensure final newline unless suppressed by patch metadata.
    if !patch.new_file_no_newline && !patch.binary {
      self.writer.ensure_newline().map_err(Error::from)?;
    }
    Ok(())
  }

  /// Process hunkless patches by sorting them based on their match position.
  fn process_hunkless_patches(
    &mut self,
    patch: &Patch<'_>,
  ) -> Result<(), Error> {
    // Hunkless patches (no @@ headers) may be out of order.
    // We sort them by their best match position in the source to ensure sequential application.
    let mut hunks_with_pos = Vec::with_capacity(patch.hunks.len());
    let matcher = Matcher;
    for hunk in patch.hunks.iter() {
      let (pos, _) = matcher.find_match(self.source_at(), hunk)?;
      hunks_with_pos.push((pos, hunk));
    }
    hunks_with_pos.sort_by_key(|(pos, _)| *pos);
    for (_, hunk) in hunks_with_pos {
      self.process_hunk(hunk)?;
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
