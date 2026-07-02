use std::io::{ErrorKind as IoErrorKind, Write};

use hex::encode_to_slice;
use memchr::memmem::Finder;
use nagato_core::{Error, ErrorKind, LineWriter};
use sha1::{Digest, Sha1};

use crate::{
  applier::matcher::Matcher, binary, BinaryKind, Hunk, LineKind, Patch,
};

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

  #[inline]
  fn write_block(&mut self, block: &[u8]) -> Result<(), Error> {
    self.writer.write_block(block).map_err(Error::from)
  }

  #[inline]
  pub fn apply_hunk<'p>(
    &mut self,
    hunk: &Hunk<'p>,
    match_pos: usize,
    remaining_source: &'s [u8],
  ) -> Result<(), Error> {
    let source = self.source_at();

    if match_pos > 0 {
      self.write_block(&source[..match_pos])?;
    }

    self.pos += source.len() - remaining_source.len();

    for line in hunk.lines.iter() {
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

    let source = self.source_at();
    let (match_pos, remaining) = Matcher.find_match(source, hunk, None)?;
    self.apply_hunk(hunk, match_pos, remaining)
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
    // Binary patches apply to the entire file state; consume remaining source to prevent trailing junk.
    self.pos = self.source.len();

    // Binary patches are applied by processing fragments until a successful literal decoding or delta application occurs.
    for fragment in patch.binary_fragments.iter() {
      if matches!(fragment.kind, BinaryKind::Literal) {
        let output = self.writer.output();
        return binary::decode_base85(&fragment.data, output);
      }

      let mut decoded = binary::new_base85_decoder(&fragment.data);
      let source = self.source;
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

  pub fn begin(&mut self, patch: &Patch<'_>) -> Result<(), Error> {
    if patch.old_hash.is_some() {
      // Skip Sha1 calculation for Literal binary patches as they overwrite the file.
      let is_literal = patch
        .binary_fragments
        .first()
        .map(|f| matches!(f.kind, BinaryKind::Literal))
        .unwrap_or(false);

      if !is_literal {
        self.verify_binary_source(patch)?;
      }
    }
    Ok(())
  }

  pub fn end(&mut self, patch: &Patch<'_>) -> Result<(), Error> {
    self.flush_remaining_source()?;
    if patch.new_file_no_newline || patch.binary {
      return Ok(());
    }
    self.writer.ensure_newline().map_err(Error::from)
  }

  pub fn process(mut self, patch: &Patch<'_>) -> Result<(), Error> {
    self.begin(patch)?;

    if !patch.binary_fragments.is_empty() {
      return self.process_binary(patch);
    }

    let Some(first_hunk) = patch.hunks.first() else {
      return self.end(patch);
    };

    if first_hunk.has_header {
      patch.hunks.iter().try_for_each(|h| self.process_hunk(h))?;
    } else {
      self.process_hunkless_patches(patch)?;
    }

    self.end(patch)
  }

  pub(crate) fn process_hunkless_patches(
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
          .first_non_empty_match_line()
          .map(|(_, l)| l.text)
          .map(Finder::new);
        Some((h, finder))
      })
      .collect();

    let mut hunks_to_apply = Vec::with_capacity(pending_hunks.len());
    let source = self.source_at();
    let initial_pos = self.pos;

    let mut current_offset = 0;

    // Check each pending hunk in order, advancing the search offset upon match.
    for hunk_opt in pending_hunks.iter_mut() {
      let Some((hunk, finder)) = hunk_opt else {
        continue;
      };

      let current_source = &source[current_offset..];
      if let Ok((match_pos, remaining)) =
        Matcher.find_match(current_source, hunk, finder.as_ref())
      {
        let absolute_pos = initial_pos + current_offset + match_pos;
        hunks_to_apply.push((absolute_pos, remaining, *hunk));
        current_offset =
          (source.len() - remaining.len()).max(current_offset + 1);
        *hunk_opt = None;
      }
    }

    // Match any remaining hunks on the entire source
    for hunk_data in pending_hunks.into_iter().flatten() {
      let (hunk, finder) = hunk_data;
      let (match_pos, remaining) =
        Matcher.find_match(source, hunk, finder.as_ref())?;
      hunks_to_apply.push((initial_pos + match_pos, remaining, hunk));
    }

    hunks_to_apply.sort_unstable_by_key(|(pos, _, _)| *pos);

    for (pos, remaining, hunk) in hunks_to_apply {
      let match_pos_rel = pos - self.pos;
      self.apply_hunk(hunk, match_pos_rel, remaining)?;
    }
    Ok(())
  }

  fn flush_remaining_source(&mut self) -> Result<(), Error> {
    let source = self.source_at();
    self.write_block(source)?;
    self.pos = self.source.len();
    Ok(())
  }
}
