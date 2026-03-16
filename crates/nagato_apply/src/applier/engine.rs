use std::io::{ErrorKind as IoErrorKind, Write};

use hex::encode_to_slice;
use memchr::memmem::Finder;
use nagato_core::{Error, ErrorKind, LineWriter};
use sha1::{Digest, Sha1};

use crate::{binary, BinaryKind, Hunk, LineKind, Matcher, Patch};

pub struct Applier<'s, 'b, W: Write + ?Sized> {
  writer: LineWriter<'b, W>,
  pub source: &'s [u8],
  pub pos: usize,
}

impl<'s, 'b, W: Write + ?Sized> Applier<'s, 'b, W> {
  #[inline]
  pub fn new(output: &'b mut W, source: &'s [u8]) -> Self {
    Self {
      writer: LineWriter::new(output),
      source,
      pos: 0,
    }
  }

  #[inline]
  fn source_at(&self) -> &'s [u8] {
    &self.source[self.pos..]
  }

  #[inline]
  pub fn write_line(&mut self, line: &[u8]) -> Result<(), Error> {
    self.writer.write_line(line).map_err(Into::into)
  }

  fn write_block(&mut self, block: &[u8]) -> Result<(), Error> {
    let mut rest = block;
    while let Some((line, next_rest)) = nagato_core::get_line(rest) {
      self.writer.write_line(line).map_err(Error::from)?;
      rest = next_rest;
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
    let match_res = matcher.find_match(source, hunk, None);

    let (match_pos, final_source, skipped_line_index) = match match_res {
      Ok((pos, src)) => (pos, src, None),
      Err(e) if hunk.has_header => return Err(e),
      Err(e) => matcher.find_match_recovery(source, hunk).map_err(|_| e)?,
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
    let old_hash = patch.old_hash;
    if old_hash.is_none() {
      return Ok(());
    }

    let old_hash_bytes = old_hash.unwrap();
    if old_hash_bytes.len() < 7 {
      return Ok(());
    }

    if old_hash_bytes.iter().all(|&b| b == b'0') {
      return Ok(());
    }

    let source = self.source_at();
    let mut hasher = Sha1::new();
    let mut len_buf = [0u8; 20];
    let len_str = {
      let mut len_writer = &mut len_buf[..];
      let _ = write!(len_writer, "{}", source.len());
      let written = 20 - len_writer.len();
      &len_buf[..written]
    };

    hasher.update(b"blob ");
    hasher.update(len_str);
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

    Ok(())
  }

  pub fn process_binary(&mut self, patch: &Patch<'_>) -> Result<(), Error> {
    // Binary patches are applied by processing fragments until a successful literal decoding or delta application occurs.
    for fragment in patch.binary_fragments.iter() {
      if matches!(fragment.kind, BinaryKind::Literal) {
        let output = self.writer.output();
        return binary::decode_base85(&fragment.data, output);
      }

      let mut decoded = binary::new_base85_decoder(&fragment.data);
      let source = self.source_at();
      let output = self.writer.output();
      let res = binary::apply_delta(&mut decoded, source, output);

      let Err(e) = res else {
        return Ok(());
      };

      if matches!(e.kind, ErrorKind::BinaryPatchSourceMismatch) {
        continue;
      }

      if matches!(e.kind, ErrorKind::Io(ref io) if io.kind() == IoErrorKind::InvalidData)
      {
        continue;
      }

      return Err(e);
    }

    Err(Error::new(ErrorKind::CouldNotApplyHunk))
  }

  pub fn process(mut self, patch: &Patch<'_>) -> Result<(), Error> {
    // Verify source hash if index header is present.
    if patch.old_hash.is_some() {
      self.verify_binary_source(patch)?
    }

    // Patch content application is dispatched to specialized handlers based on whether the patch contains binary fragments or standard text hunks.
    if !patch.binary_fragments.is_empty() {
      return self.process_binary(patch);
    }

    // Patch hunks are processed either as a hunkless collection or individually if headers are present.
    let Some(first_hunk) = patch.hunks.first() else {
      self.flush_remaining_source()?;
      if patch.new_file_no_newline || patch.binary {
        return Ok(());
      }
      return self.writer.ensure_newline().map_err(Error::from);
    };

    if first_hunk.has_header {
      patch.hunks.iter().try_for_each(|h| self.process_hunk(h))?;
    } else {
      self.process_hunkless_patches(patch)?;
    }

    // Write any remaining source content to the output.
    self.flush_remaining_source()?;

    // Ensure final newline unless suppressed by patch metadata.
    if patch.new_file_no_newline || patch.binary {
      return Ok(());
    }

    self.writer.ensure_newline().map_err(Error::from)?;
    Ok(())
  }

  fn process_hunkless_patches(
    &mut self,
    patch: &Patch<'_>,
  ) -> Result<(), Error> {
    // Hunkless patches are often used in "diff-lite" formats where headers are missing.
    // Pre-compute Finders for all hunks to speed up search.
    let mut pending_hunks: Vec<Option<(&Hunk<'_>, Option<Finder>)>> = patch
      .hunks
      .iter()
      .map(|h| {
        let finder = h
          .lines_to_match()
          .next()
          .map(|(_, l)| l.text)
          .filter(|t| !t.is_empty())
          .map(Finder::new);
        Some((h, finder))
      })
      .collect();

    let mut hunks_with_pos = Vec::with_capacity(pending_hunks.len());
    let source = self.source_at();

    let mut current_pos = 0;
    let mut remaining_count = pending_hunks.len();

    while remaining_count > 0 && current_pos < source.len() {
      let mut found_idx = None;
      let current_source = &source[current_pos..];
      for (i, hunk_opt) in pending_hunks.iter_mut().enumerate() {
        let Some((hunk, finder)) = hunk_opt else {
          continue;
        };

        let Ok((match_pos, _)) =
          Matcher.find_match(current_source, hunk, finder.as_ref())
        else {
          continue;
        };

        hunks_with_pos.push((current_pos + match_pos, *hunk));
        found_idx = Some(i);
        break;
      }

      let Some(i) = found_idx else {
        break;
      };

      let (pos, hunk) = hunks_with_pos
        .last()
        .expect("hunks_with_pos is empty after found_idx");
      let mut match_len = 0;
      for line in hunk.lines.iter() {
        if !matches!(line.kind, LineKind::Addition) {
          match_len += line.text.len() + 1;
        }
      }

      current_pos = *pos + match_len;
      pending_hunks[i] = None;
      remaining_count -= 1;
    }

    for hunk_data in pending_hunks.into_iter().flatten() {
      let (hunk, finder) = hunk_data;
      let (pos, _) = Matcher.find_match(source, hunk, finder.as_ref())?;
      hunks_with_pos.push((pos, hunk));
    }

    hunks_with_pos.sort_unstable_by_key(|(pos, _)| *pos);
    hunks_with_pos
      .into_iter()
      .try_for_each(|(_, hunk)| self.process_hunk(hunk))
  }

  fn flush_remaining_source(&mut self) -> Result<(), Error> {
    let source = self.source_at();
    self.write_block(source)?;
    self.pos = self.source.len();
    Ok(())
  }
}
