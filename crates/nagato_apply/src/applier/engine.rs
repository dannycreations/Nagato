use std::io::Write;

use bstr::ByteSlice;
use nagato_core::{Error, ErrorKind, LineWriter};
use sha1::{Digest, Sha1};

use crate::{binary, BinaryKind, Hunk, LineKind, Matcher, Patch};

pub struct Applier<'s, 'b, W: Write + ?Sized> {
  writer: LineWriter<'b, W>,
  pub source: &'s [u8],
  pub pos: usize,
}

impl<'s, 'b, W: Write + ?Sized> Applier<'s, 'b, W> {
  pub fn new(output: &'b mut W, source: &'s [u8]) -> Self {
    Self {
      writer: LineWriter::new(output),
      source,
      pos: 0,
    }
  }

  #[inline(always)]
  fn source_at(&self) -> &'s [u8] {
    &self.source[self.pos..]
  }

  #[inline]
  pub fn write_line(&mut self, line: &[u8]) -> Result<(), Error> {
    self.writer.write_line(line).map_err(Into::into)
  }

  fn write_block(&mut self, block: &[u8]) -> Result<(), Error> {
    block.lines().try_for_each(|l| self.write_line(l))
  }

  #[inline]
  pub fn find_and_apply_hunk<'p>(
    &mut self,
    hunk: &Hunk<'p>,
  ) -> Result<(), Error> {
    let source = self.source_at();
    let matcher = Matcher;

    // Hunk matching logic uses explicit pattern matching to handle recovery attempts for hunkless patches when an exact match fails.
    let (match_pos, final_source, skipped_line_index) =
      match matcher.find_match(source, hunk) {
        Ok((pos, src)) => (pos, src, None),
        Err(_) if !hunk.has_header => {
          matcher.find_match_recovery(source, hunk)?
        }
        Err(e) => return Err(e),
      };
    let skipped = &source[..match_pos];
    self.write_block(skipped)?;
    self.pos += source.len() - final_source.len();

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

  pub fn verify_binary_source(&self, patch: &Patch<'_>) -> Result<(), Error> {
    if let Some(old_hash_bytes) = patch
      .old_hash
      .filter(|h| h.len() >= 7 && h.iter().any(|&b| b != b'0'))
    {
      let source = self.source_at();
      let mut hasher = Sha1::new();
      let _ = write!(hasher, "blob {}\0", source.len());
      hasher.update(source);

      let mut hex_hash = [0u8; 40];
      hex::encode_to_slice(hasher.finalize(), &mut hex_hash).unwrap();

      if !hex_hash.starts_with(old_hash_bytes) {
        return Err(Error::new(ErrorKind::BinaryPatchSourceMismatch));
      }
    }
    Ok(())
  }

  pub fn process_binary(&mut self, patch: &Patch<'_>) -> Result<(), Error> {
    // Binary patches are applied by processing fragments until a successful literal decoding or delta application occurs.
    patch
      .binary_fragments
      .iter()
      .find_map(|fragment| match fragment.kind {
        BinaryKind::Literal => {
          Some(binary::decode_base85(&fragment.data, self.writer.output()))
        }
        BinaryKind::Delta => {
          let mut decoded = binary::new_base85_decoder(&fragment.data);
          match binary::apply_delta(
            &mut decoded,
            self.source_at(),
            self.writer.output(),
          ) {
            Ok(_) => Some(Ok(())),
            Err(e)
              if matches!(e.kind, ErrorKind::BinaryPatchSourceMismatch) =>
            {
              None
            }
            Err(e) => Some(Err(e)),
          }
        }
      })
      .unwrap_or(Err(Error::new(ErrorKind::CouldNotApplyHunk)))
  }

  pub fn process(mut self, patch: &Patch<'_>) -> Result<(), Error> {
    // Verify source hash if index header is present.
    self.verify_binary_source(patch)?;

    // Patch content application is dispatched to specialized handlers based on whether the patch contains binary fragments or standard text hunks.
    if !patch.binary_fragments.is_empty() {
      return self.process_binary(patch);
    }

    // Patch hunks are processed either as a hunkless collection or individually if headers are present.
    match patch.hunks.first() {
      Some(h) if !h.has_header => self.process_hunkless_patches(patch)?,
      _ => patch.hunks.iter().try_for_each(|h| self.process_hunk(h))?,
    }

    // Write any remaining source content to the output.
    self.flush_remaining_source()?;

    // Ensure final newline unless suppressed by patch metadata.
    if !patch.new_file_no_newline && !patch.binary {
      self.writer.ensure_newline().map_err(Error::from)?;
    }
    Ok(())
  }

  fn process_hunkless_patches(
    &mut self,
    patch: &Patch<'_>,
  ) -> Result<(), Error> {
    // Hunkless patches are mapped to their best match positions and sorted to ensure sequential application when headers are missing.
    let mut hunks_with_pos = patch
      .hunks
      .iter()
      .map(|hunk| {
        Matcher
          .find_match(self.source_at(), hunk)
          .map(|(pos, _)| (pos, hunk))
      })
      .collect::<Result<Vec<_>, _>>()?;

    hunks_with_pos.sort_by_key(|(pos, _)| *pos);
    for (_, hunk) in hunks_with_pos {
      self.process_hunk(hunk)?;
    }
    Ok(())
  }

  fn flush_remaining_source(&mut self) -> Result<(), Error> {
    let source = self.source_at();
    if !source.is_empty() {
      self.write_block(source)?;
      self.pos = self.source.len();
    }
    Ok(())
  }
}
