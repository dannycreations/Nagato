use std::io::{ErrorKind as IoErrorKind, Write};

use memchr::memmem::Finder;
use nagato_core::{Error, ErrorKind, LineWriter};
use sha1::{Digest, Sha1};

use crate::{
  applier::matcher::{first_non_empty_match_line, Matcher},
  binary, BinaryKind, Hunk, Line, LineKind, Patch,
};

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

fn hex_starts_with(digest: &[u8], prefix: &[u8]) -> bool {
  if prefix.len() > digest.len() * 2 {
    return false;
  }

  prefix.iter().enumerate().all(|(i, &c)| {
    let byte = digest[i / 2];
    let nibble = if i % 2 == 0 { byte >> 4 } else { byte & 0x0f };
    c == HEX_DIGITS[nibble as usize]
  })
}

fn format_usize(buf: &mut [u8; 20], value: usize) -> &[u8] {
  let capacity = buf.len();
  let written = {
    let mut tail = &mut buf[..];
    let _ = write!(tail, "{value}");
    capacity - tail.len()
  };
  &buf[..written]
}

pub struct Applier<'s, 'b, W: Write + ?Sized> {
  writer: LineWriter<'b, W>,
  source: &'s [u8],
  pos: usize,
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

  pub fn process(mut self, patch: &Patch<'_>) -> Result<(), Error> {
    self.begin(patch)?;

    if !patch.binary_fragments.is_empty() {
      return self.process_binary(patch);
    }

    let Some(first_hunk) = patch.hunks.first() else {
      return self.end(patch);
    };

    if first_hunk.has_header {
      patch
        .hunks
        .iter()
        .try_for_each(|h| self.process_hunk(patch, h))?;
    } else {
      self.process_hunkless_patches(patch)?;
    }

    self.end(patch)
  }

  pub fn process_binary(&mut self, patch: &Patch<'_>) -> Result<(), Error> {
    // Binary patches apply to the entire file state; consume remaining source to prevent trailing junk.
    self.pos = self.source.len();

    // Binary patches are applied by processing fragments until a successful literal decoding or delta application occurs.
    for fragment in patch.binary_fragments.iter() {
      let data = patch.binary_fragment_data(fragment);
      if matches!(fragment.kind, BinaryKind::Literal) {
        return binary::decode_base85(data, self.writer.output());
      }

      let mut decoded = binary::new_base85_decoder(data);
      let res =
        binary::apply_delta(&mut decoded, self.source, self.writer.output());

      let Err(e) = res else {
        return Ok(());
      };

      // A fragment that does not describe this source is not fatal; git emits
      // both directions and only one of them applies.
      let is_wrong_fragment =
        matches!(e.kind, ErrorKind::BinaryPatchSourceMismatch)
          || matches!(
            e.kind,
            ErrorKind::Io(ref io) if io.kind() == IoErrorKind::InvalidData
          );
      if !is_wrong_fragment {
        return Err(e);
      }
    }

    Err(Error::new(ErrorKind::CouldNotApplyHunk))
  }

  pub(crate) fn begin(&mut self, patch: &Patch<'_>) -> Result<(), Error> {
    // Literal binary patches overwrite the file wholesale, so the source hash
    // is irrelevant and the Sha1 pass can be skipped.
    let is_literal = patch
      .binary_fragments
      .first()
      .is_some_and(|f| matches!(f.kind, BinaryKind::Literal));

    if is_literal {
      return Ok(());
    }
    self.verify_binary_source(patch)
  }

  pub(crate) fn end(&mut self, patch: &Patch<'_>) -> Result<(), Error> {
    self.flush_remaining_source()?;
    if patch.new_file_no_newline || patch.binary {
      return Ok(());
    }
    self.writer.ensure_newline().map_err(Error::from)
  }

  pub(crate) fn process_hunk<'p>(
    &mut self,
    patch: &Patch<'p>,
    hunk: &Hunk<'p>,
  ) -> Result<(), Error> {
    let lines = patch.hunk_lines(hunk);
    if hunk.old_span == 0 {
      for line in lines {
        if matches!(line.kind, LineKind::Addition) {
          self.write_line(line.text)?;
        }
      }
      return Ok(());
    }

    let source = self.source_at();
    let (match_pos, remaining) =
      Matcher.find_match(source, patch, hunk, None)?;
    self.apply_hunk(lines, match_pos, remaining)
  }

  pub(crate) fn process_hunkless_patches(
    &mut self,
    patch: &Patch<'_>,
  ) -> Result<(), Error> {
    // Hunkless patches are often used in "diff-lite" formats where headers are missing.
    // Pre-compute Finders for all hunks to speed up search.
    let mut pending: Vec<Option<(&Hunk<'_>, Option<Finder>)>> = patch
      .hunks
      .iter()
      .map(|h| {
        let lines = patch.hunk_lines(h);
        let finder =
          first_non_empty_match_line(lines).map(|(_, l)| Finder::new(l.text));
        Some((h, finder))
      })
      .collect();

    let mut to_apply = Vec::with_capacity(pending.len());
    let source = self.source_at();
    let initial_pos = self.pos;
    let mut offset = 0;

    // First pass: match hunks in document order, advancing past each match so
    // repeated content binds to successive occurrences.
    for slot in pending.iter_mut() {
      let Some((hunk, finder)) = slot else {
        continue;
      };
      if offset > source.len() {
        continue;
      }

      let Ok((match_pos, remaining)) =
        Matcher.find_match(&source[offset..], patch, hunk, finder.as_ref())
      else {
        continue;
      };

      to_apply.push((initial_pos + offset + match_pos, remaining, *hunk));
      offset = (source.len() - remaining.len()).max(offset + 1);
      *slot = None;
    }

    // Second pass: anything left over is matched against the whole source.
    for (hunk, finder) in pending.into_iter().flatten() {
      let (match_pos, remaining) =
        Matcher.find_match(source, patch, hunk, finder.as_ref())?;
      to_apply.push((initial_pos + match_pos, remaining, hunk));
    }

    to_apply.sort_unstable_by_key(|(pos, _, _)| *pos);

    for (pos, remaining, hunk) in to_apply {
      let lines = patch.hunk_lines(hunk);
      self.apply_hunk(lines, pos - self.pos, remaining)?;
    }
    Ok(())
  }

  fn verify_binary_source(&self, patch: &Patch<'_>) -> Result<(), Error> {
    let Some(expected) = patch.old_hash else {
      return Ok(());
    };

    // Abbreviations shorter than 7 chars are too weak to check, and an
    // all-zero hash marks a newly created file.
    if expected.len() < 7 || expected.iter().all(|&b| b == b'0') {
      return Ok(());
    }

    let source = self.source_at();
    let mut len_buf = [0u8; 20];

    // Git hashes blobs as "blob <length>\0<content>".
    let mut hasher = Sha1::new();
    hasher.update(b"blob ");
    hasher.update(format_usize(&mut len_buf, source.len()));
    hasher.update(b"\0");
    hasher.update(source);

    if !hex_starts_with(&hasher.finalize(), expected) {
      return Err(Error::new(ErrorKind::BinaryPatchSourceMismatch));
    }
    Ok(())
  }

  #[inline]
  fn apply_hunk(
    &mut self,
    lines: &[Line<'_>],
    match_pos: usize,
    remaining_source: &'s [u8],
  ) -> Result<(), Error> {
    let source = self.source_at();

    if match_pos > 0 {
      self.write_block(&source[..match_pos])?;
    }

    self.pos += source.len() - remaining_source.len();

    for line in lines {
      match line.kind {
        LineKind::Addition | LineKind::Context | LineKind::Gap => {
          self.write_line(line.text)?;
        }
        LineKind::Deletion => {}
      }
    }

    Ok(())
  }

  fn flush_remaining_source(&mut self) -> Result<(), Error> {
    let source = self.source_at();
    self.write_block(source)?;
    self.pos = self.source.len();
    Ok(())
  }

  #[inline]
  fn source_at(&self) -> &'s [u8] {
    &self.source[self.pos..]
  }

  #[inline]
  fn write_line(&mut self, line: &[u8]) -> Result<(), Error> {
    self.writer.write_line(line).map_err(Error::from)
  }

  #[inline]
  fn write_block(&mut self, block: &[u8]) -> Result<(), Error> {
    self.writer.write_block(block).map_err(Error::from)
  }
}
