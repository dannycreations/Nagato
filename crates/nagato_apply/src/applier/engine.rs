use std::io::Write;

use hex::encode_to_slice;
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
    if block.is_empty() {
      return Ok(());
    }

    let mut remaining = block;
    while !remaining.is_empty() {
      let (line, rest) = match memchr::memchr(b'\n', remaining) {
        Some(idx) => (&remaining[..idx], &remaining[idx + 1..]),
        None => (remaining, &[][..]),
      };

      let line = line.strip_suffix(b"\r").unwrap_or(line);
      self.write_line(line)?;
      remaining = rest;
    }
    Ok(())
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
        Err(e) if !hunk.has_header => {
          matcher.find_match_recovery(source, hunk).map_err(|_| e)?
        }
        Err(e) => return Err(e),
      };

    if match_pos > 0 {
      self.write_block(&source[..match_pos])?;
    }
    self.pos += source.len() - final_source.len();

    for (i, line) in hunk.lines.iter().enumerate() {
      if Some(i) == skipped_line_index {
        continue;
      }
      match line.kind {
        LineKind::Addition | LineKind::Context | LineKind::Gap => {
          self.write_line(line.text)?;
        }
        LineKind::Deletion => {}
      }
    }

    Ok(())
  }

  pub fn process_hunk<'p>(&mut self, hunk: &Hunk<'p>) -> Result<(), Error> {
    if hunk.old_span == 0 {
      for line in hunk.lines.iter() {
        if matches!(line.kind, LineKind::Addition) {
          self.write_line(line.text)?;
        }
      }
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
      hasher.update(b"blob ");
      hasher.update(source.len().to_string().as_bytes());
      hasher.update(b"\0");
      hasher.update(source);

      let mut hex_hash = [0u8; 40];
      // SAFETY: 20 bytes Sha1 hash always results in 40 bytes hex.
      // This is a fixed-size buffer operation with zero risk of overflow or indexing errors.
      encode_to_slice(hasher.finalize(), &mut hex_hash)
        .expect("fixed-size hex encoding failed");

      if !hex_hash.starts_with(old_hash_bytes) {
        return Err(Error::new(ErrorKind::BinaryPatchSourceMismatch));
      }
    }
    Ok(())
  }

  pub fn process_binary(&mut self, patch: &Patch<'_>) -> Result<(), Error> {
    // Binary patches are applied by processing fragments until a successful literal decoding or delta application occurs.
    for fragment in patch.binary_fragments.iter() {
      let res = match fragment.kind {
        BinaryKind::Literal => {
          binary::decode_base85(&fragment.data, self.writer.output())
        }
        BinaryKind::Delta => {
          let mut decoded = binary::new_base85_decoder(&fragment.data);
          match binary::apply_delta(
            &mut decoded,
            self.source_at(),
            self.writer.output(),
          ) {
            Ok(_) => Ok(()),
            Err(e)
              if matches!(e.kind, ErrorKind::BinaryPatchSourceMismatch) =>
            {
              continue;
            }
            Err(e) => Err(e),
          }
        }
      };

      return match res {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
      };
    }

    Err(Error::new(ErrorKind::CouldNotApplyHunk))
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
    let mut pending_hunks: Vec<_> = patch.hunks.iter().collect();
    let mut hunks_with_pos = Vec::with_capacity(pending_hunks.len());
    let source = self.source_at();

    let mut current_pos = 0;
    while !pending_hunks.is_empty() && current_pos < source.len() {
      let mut found_idx = None;
      for (i, hunk) in pending_hunks.iter().enumerate() {
        if let Ok((match_pos, _)) =
          Matcher.find_match(&source[current_pos..], hunk)
        {
          let absolute_pos = current_pos + match_pos;
          hunks_with_pos.push((absolute_pos, *hunk));
          found_idx = Some(i);
          break;
        }
      }

      if let Some(i) = found_idx {
        let (pos, _) = hunks_with_pos.last().unwrap();
        let matched_text = &source[*pos..];
        let next_line_pos = match memchr::memchr(b'\n', matched_text) {
          Some(idx) => idx + 1,
          None => matched_text.len(),
        };
        current_pos = *pos + next_line_pos;
        pending_hunks.remove(i);
      } else {
        break;
      }
    }

    if !pending_hunks.is_empty() {
      for hunk in pending_hunks {
        let (pos, _) = Matcher.find_match(source, hunk)?;
        hunks_with_pos.push((pos, hunk));
      }
    }

    hunks_with_pos.sort_unstable_by_key(|(pos, _)| *pos);
    hunks_with_pos
      .into_iter()
      .try_for_each(|(_, hunk)| self.process_hunk(hunk))
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
