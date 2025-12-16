use std::{
  io::{self, sink, Write},
  mem,
};

use bstr::ByteSlice;
use memchr::{memchr, memchr_iter, memmem};
use memmap2::Mmap;
use nagato_core::{
  error::{Error, ErrorKind},
  fs::FileSystem,
};
use sha1::{Digest, Sha1};

use crate::{binary, BinaryPatchKind, Hunk, Line, LineKind, Patch};

impl<'a> Patch<'a> {
  pub fn invert(mut self) -> Self {
    let is_creation = self.old_file == b"/dev/null";
    let is_deletion = self.new_file == b"/dev/null";

    mem::swap(&mut self.old_file, &mut self.new_file);
    mem::swap(&mut self.rename_from, &mut self.rename_to);
    mem::swap(&mut self.copy_from, &mut self.copy_to);
    mem::swap(&mut self.old_file_no_newline, &mut self.new_file_no_newline);
    mem::swap(&mut self.old_hash, &mut self.new_hash);

    if is_creation {
      self.deleted_mode = self.new_mode;
      self.new_mode = None;
      self.old_mode = None;
    } else if is_deletion {
      self.new_mode = self.deleted_mode.or(self.old_mode);
      self.old_mode = None;
      self.deleted_mode = None;
    } else {
      mem::swap(&mut self.old_mode, &mut self.new_mode);
    }

    self.hunks.iter_mut().for_each(Hunk::invert);
    self
  }
}

impl<'a> Hunk<'a> {
  pub(crate) fn invert(&mut self) {
    mem::swap(&mut self.old_line, &mut self.new_line);
    mem::swap(&mut self.old_span, &mut self.new_span);
    self.lines.iter_mut().for_each(|line| {
      line.kind = match line.kind {
        LineKind::Addition => LineKind::Deletion,
        LineKind::Deletion => LineKind::Addition,
        LineKind::Context => LineKind::Context,
      };
    });
  }
}

#[inline(always)]
fn get_line(source: &[u8]) -> Option<(&[u8], &[u8])> {
  if source.is_empty() {
    return None;
  }
  let end = source.find_byte(b'\n').unwrap_or(source.len());
  let full_line = &source[..end];
  let next_source = if end < source.len() {
    &source[end + 1..]
  } else {
    &[]
  };
  let line_content = full_line.strip_suffix(b"\r").unwrap_or(full_line);
  Some((line_content, next_source))
}

struct Applier<'s, 'b, W: Write + ?Sized> {
  output: &'b mut W,
  source: &'s [u8],
  is_at_start_of_file: bool,
  current_source_line: u32,
}

impl<'s, 'b, W: Write + ?Sized> Applier<'s, 'b, W> {
  fn new(output: &'b mut W, source: &'s [u8]) -> Self {
    Self {
      output,
      source,
      is_at_start_of_file: true,
      current_source_line: 0,
    }
  }

  #[inline]
  fn write_line(&mut self, line: &[u8]) -> Result<(), Error> {
    if !self.is_at_start_of_file {
      self.output.write_all(b"\n")?;
    }
    self.is_at_start_of_file = false;
    self.output.write_all(line)?;
    Ok(())
  }

  #[inline]
  fn consume_line(&mut self) -> Option<&'s [u8]> {
    let (line, next_source) = get_line(self.source)?;
    self.source = next_source;
    Some(line)
  }

  fn advance_to_hunk(&mut self, hunk: &Hunk) -> Result<(), Error> {
    let target_line = hunk.old_line.saturating_sub(1);

    // Bulk skip lines if possible
    let lines_to_skip = target_line.saturating_sub(self.current_source_line);
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

      if count == lines_to_skip {
        let block = &self.source[..end_offset];
        // Only use bulk write if no CR, to preserve normalization behavior
        if memchr(b'\r', block).is_none() {
          if !self.is_at_start_of_file {
            self.output.write_all(b"\n")?;
          }
          // block ends with \n, strip it to match write_line behavior
          if !block.is_empty() {
            self.output.write_all(&block[..block.len() - 1])?;
          }
          self.is_at_start_of_file = false;

          self.source = &self.source[end_offset..];
          self.current_source_line += count;
          return Ok(());
        }
      }
    }

    while self.current_source_line < target_line {
      if let Some(line) = self.consume_line() {
        self.write_line(line)?;
        self.current_source_line += 1;
      } else {
        if hunk.old_span > 0 {
          return Err(Error {
            line: Some(hunk.patch_line_num),
            kind: ErrorKind::CouldNotApplyHunk,
          });
        }
        break;
      }
    }
    Ok(())
  }

  #[inline]
  fn verify_match<'p>(
    &self,
    mut source: &'s [u8],
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
    hunk: &Hunk,
  ) -> Result<&'s [u8], Error> {
    for (offset, hunk_line) in lines_to_match {
      let expected = hunk_line.text;
      let len = expected.len();

      if source.len() < len || &source[..len] != expected {
        return Err(Error {
          line: Some(hunk.patch_line_num + 1 + offset as u32),
          kind: ErrorKind::CouldNotApplyHunk,
        });
      }

      let after = &source[len..];
      if after.is_empty() {
        source = after;
      } else if after[0] == b'\n' {
        source = &after[1..];
      } else if after[0] == b'\r' {
        if after.get(1) == Some(&b'\n') {
          source = &after[2..];
        } else if after.len() == 1 {
          source = &after[1..];
        } else {
          return Err(Error {
            line: Some(hunk.patch_line_num + 1 + offset as u32),
            kind: ErrorKind::CouldNotApplyHunk,
          });
        }
      } else {
        return Err(Error {
          line: Some(hunk.patch_line_num + 1 + offset as u32),
          kind: ErrorKind::CouldNotApplyHunk,
        });
      }
    }
    Ok(source)
  }

  fn find_hunk_match<'p>(
    &mut self,
    hunk: &Hunk<'p>,
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
    first_line_to_match: &Line,
  ) -> Result<(), Error> {
    let needle = first_line_to_match.text;

    if needle.is_empty() {
      loop {
        let (line, next_source) = if let Some(res) = get_line(self.source) {
          res
        } else {
          return Err(Error {
            line: Some(hunk.patch_line_num),
            kind: ErrorKind::CouldNotApplyHunk,
          });
        };

        if line != needle {
          if let Some(line) = self.consume_line() {
            self.write_line(line)?;
            self.current_source_line += 1;
            continue;
          } else {
            unreachable!("We just peeked it");
          }
        }

        let result =
          self.verify_match(next_source, lines_to_match.clone(), hunk);
        if let Ok(final_source) = result {
          self.source = final_source;
          return Ok(());
        }
        return result.map(|_| ());
      }
    }

    let finder = memmem::Finder::new(needle);

    for match_pos in finder.find_iter(self.source) {
      if match_pos > 0 && self.source[match_pos - 1] != b'\n' {
        continue;
      }

      let end_pos = match_pos + needle.len();
      let (is_end, next_start) = if end_pos == self.source.len() {
        (true, end_pos)
      } else if self.source[end_pos] == b'\n' {
        (true, end_pos + 1)
      } else if self.source[end_pos] == b'\r'
        && end_pos + 1 < self.source.len()
        && self.source[end_pos + 1] == b'\n'
      {
        (true, end_pos + 2)
      } else {
        (false, 0)
      };

      if !is_end {
        continue;
      }

      let next_source = &self.source[next_start..];
      let result = self.verify_match(next_source, lines_to_match.clone(), hunk);

      if let Ok(final_source) = result {
        let skipped = &self.source[..match_pos];

        // Bulk write skipped lines if possible
        if !skipped.is_empty() && memchr(b'\r', skipped).is_none() {
          let lines_skipped = memchr_iter(b'\n', skipped).count() as u32;

          if !self.is_at_start_of_file {
            self.output.write_all(b"\n")?;
          }
          // skipped ends with \n because match is at line boundary
          if !skipped.is_empty() {
            self.output.write_all(&skipped[..skipped.len() - 1])?;
          }
          self.is_at_start_of_file = false;

          self.current_source_line += lines_skipped;
        } else {
          for line in skipped.lines() {
            self.write_line(line)?;
            self.current_source_line += 1;
          }
        }

        self.source = final_source;
        self.current_source_line += 1;

        return Ok(());
      }
      return result.map(|_| ());
    }

    Err(Error {
      line: Some(hunk.patch_line_num),
      kind: ErrorKind::CouldNotApplyHunk,
    })
  }

  fn find_and_apply_hunk<'p>(&mut self, hunk: &Hunk<'p>) -> Result<(), Error> {
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

    self.current_source_line += hunk.old_span - 1;

    for line in &hunk.lines {
      match line.kind {
        LineKind::Addition | LineKind::Context => self.write_line(line.text)?,
        LineKind::Deletion => {}
      }
    }

    Ok(())
  }

  fn process_hunk<'p>(&mut self, hunk: &Hunk<'p>) -> Result<(), Error> {
    self.advance_to_hunk(hunk)?;

    if hunk.old_span == 0 {
      for line in &hunk.lines {
        if matches!(line.kind, LineKind::Addition) {
          self.write_line(line.text)?;
        }
      }
      return Ok(());
    }

    self.find_and_apply_hunk(hunk)
  }

  fn process(mut self, patch: &Patch<'_>) -> Result<(), Error> {
    if !patch.binary_fragments.is_empty() {
      if let Some(old_hash_bytes) = patch.old_hash {
        if old_hash_bytes.len() >= 7 {
          let mut hasher = Sha1::new();
          hasher.update(b"blob ");
          hasher.update(self.source.len().to_string().as_bytes());
          hasher.update(b"\0");
          hasher.update(self.source);
          let result = hasher.finalize();
          let hex_hash = hex::encode(result);

          let old_hash_str =
            std::str::from_utf8(old_hash_bytes).map_err(|_| Error {
              line: None,
              kind: ErrorKind::InvalidIndexLine,
            })?;

          if !hex_hash.starts_with(old_hash_str)
            && old_hash_str.chars().any(|c| c != '0')
          {
            return Err(Error {
              line: None,
              kind: ErrorKind::BinaryPatchSourceMismatch,
            });
          }
        }
      }

      let mut applied = false;
      for fragment in &patch.binary_fragments {
        match fragment.kind {
          BinaryPatchKind::Literal => {
            binary::decode_base85(&fragment.data, &mut self.output)?;
            applied = true;
            break;
          }
          BinaryPatchKind::Delta => {
            let mut decoded = binary::new_base85_decoder(&fragment.data);
            match binary::apply_delta(
              &mut decoded,
              self.source,
              &mut self.output,
            ) {
              Ok(_) => {
                applied = true;
                break;
              }
              Err(Error {
                kind: ErrorKind::BinaryPatchSourceMismatch,
                ..
              }) => {
                continue;
              }
              Err(e) => return Err(e),
            }
          }
        }
      }

      if !applied {
        return Err(Error {
          line: None,
          kind: ErrorKind::CouldNotApplyHunk,
        });
      }
      return Ok(());
    }

    for hunk in &patch.hunks {
      self.process_hunk(hunk)?;
    }

    while let Some(line) = self.consume_line() {
      self.write_line(line)?;
    }

    if !patch.new_file_no_newline && !patch.binary && !self.is_at_start_of_file
    {
      self.output.write_all(b"\n")?;
    }
    Ok(())
  }
}

pub fn apply<'a>(
  output: &mut (impl Write + ?Sized),
  patch: &Patch<'a>,
  source: &[u8],
) -> Result<(), Error> {
  if patch.hunks.is_empty()
    && patch.copy_to.is_none()
    && patch.binary_fragments.is_empty()
  {
    output.write_all(source)?;
    return Ok(());
  }
  Applier::new(output, source).process(patch)
}

fn ignore_not_found(res: Result<(), Error>) -> Result<(), Error> {
  match res {
    Err(Error {
      kind: ErrorKind::Io(e),
      ..
    }) if e.kind() == io::ErrorKind::NotFound => Ok(()),
    res => res,
  }
}

fn read_source_or_empty(
  fs: &FileSystem,
  path: &[u8],
) -> Result<Option<Mmap>, Error> {
  if path == b"/dev/null" {
    return Ok(None);
  }
  match fs.read(path) {
    Ok(mmap) => Ok(Some(mmap)),
    Err(Error {
      kind: ErrorKind::Io(e),
      ..
    }) if e.kind() == io::ErrorKind::NotFound => Ok(None),
    Err(e) => Err(e),
  }
}

fn handle_file_deletion(
  fs: &FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  let source = read_source_or_empty(fs, source_path)?;
  let source_slice = source.as_deref().unwrap_or(&[]);
  apply(&mut sink(), patch, source_slice)?;

  ignore_not_found(fs.remove_file(source_path))?;
  Ok(())
}

fn handle_metadata_change(
  fs: &FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  if patch.rename_to.is_some() {
    fs.rename(source_path, patch.new_file)?;
  } else if patch.copy_to.is_some() {
    fs.copy(source_path, patch.new_file)?;
  } else if patch.old_file == b"/dev/null" {
    fs.write(patch.new_file)?.commit()?;
  }
  Ok(())
}

fn handle_content_change(
  fs: &FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  let mut writer = fs.write(patch.new_file)?;
  {
    let source = read_source_or_empty(fs, source_path)?;
    let source_slice = source.as_deref().unwrap_or(&[]);
    apply(&mut writer, patch, source_slice)?;
  }
  writer.commit()?;

  if patch.rename_to.is_some() && source_path != patch.new_file {
    ignore_not_found(fs.remove_file(source_path))?;
  }
  Ok(())
}

fn patch_file_worker(fs: &FileSystem, patch: &Patch<'_>) -> Result<(), Error> {
  if patch.binary && !patch.hunks.is_empty() {
    return Err(Error {
      line: None,
      kind: ErrorKind::UnsupportedBinaryPatch,
    });
  }

  if !patch.binary_fragments.is_empty() {
    return handle_content_change(fs, patch);
  }

  match (patch.new_file, patch.hunks.is_empty()) {
    (b"/dev/null", _) => handle_file_deletion(fs, patch)?,
    (_, true) => handle_metadata_change(fs, patch)?,
    (_, false) => handle_content_change(fs, patch)?,
  }

  if patch.new_file != b"/dev/null" {
    if let Some(mode) = patch.new_mode.or(patch.index_mode) {
      fs.set_permissions(patch.new_file, mode)?;
    }
  }

  Ok(())
}

pub fn patch_file(
  fs: &FileSystem,
  patch: Patch<'_>,
  reverse: bool,
) -> Result<(), Error> {
  if reverse {
    patch_file_worker(fs, &patch.invert())
  } else {
    patch_file_worker(fs, &patch)
  }
}
