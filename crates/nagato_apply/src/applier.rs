use std::{
  io::{self, Write},
  mem,
};

use bstr::{ByteSlice, Lines};
use nagato_core::{
  error::{Error, ErrorKind},
  fs::FileSystem,
};

use crate::{Hunk, LineKind, Patch};

impl<'a> Patch<'a> {
  pub fn invert(mut self) -> Self {
    // Determine the patch type before making changes. This avoids relying on a
    // partially-mutated state, making the logic clearer and less error-prone.
    let is_creation = self.old_file == b"/dev/null";
    let is_deletion = self.new_file == b"/dev/null";

    // Invert file paths and metadata.
    mem::swap(&mut self.old_file, &mut self.new_file);
    mem::swap(&mut self.rename_from, &mut self.rename_to);
    mem::swap(&mut self.copy_from, &mut self.copy_to);
    mem::swap(&mut self.old_file_no_newline, &mut self.new_file_no_newline);

    if is_creation {
      // Inverting a creation results in a deletion.
      // The creation's `new_mode` becomes the deletion's `deleted_mode`.
      self.deleted_mode = self.new_mode;
      self.new_mode = None;
      self.old_mode = None; // A creation has no old_mode.
    } else if is_deletion {
      // Inverting a deletion results in a creation.
      // The deletion's `deleted_mode` becomes the creation's `new_mode`.
      self.new_mode = self.deleted_mode.or(self.old_mode);
      self.old_mode = None;
      self.deleted_mode = None;
    } else {
      // Inverting a modification swaps the modes.
      mem::swap(&mut self.old_mode, &mut self.new_mode);
    }

    self.hunks.iter_mut().for_each(Hunk::invert);
    self
  }
}

impl<'a> Hunk<'a> {
  pub(crate) fn invert(&mut self) {
    // Inverting a hunk means swapping old and new line numbers and spans.
    mem::swap(&mut self.old_line, &mut self.new_line);
    mem::swap(&mut self.old_span, &mut self.new_span);
    // The `invert` logic now correctly handles the `Line.kind` field,
    // swapping additions and deletions while leaving context lines untouched.
    self.lines.iter_mut().for_each(|line| {
      line.kind = match line.kind {
        LineKind::Addition => LineKind::Deletion,
        LineKind::Deletion => LineKind::Addition,
        LineKind::Context => LineKind::Context,
      };
    });
  }
}

struct Applier<'s, 'b, W: Write + ?Sized> {
  output: &'b mut W,
  source: Lines<'s>,
  is_at_start_of_file: bool,
  current_source_line: u32,
}

impl<'s, 'b, W: Write + ?Sized> Applier<'s, 'b, W> {
  fn new(output: &'b mut W, source: &'s [u8]) -> Self {
    Self {
      output,
      source: source.lines(),
      is_at_start_of_file: true,
      current_source_line: 0,
    }
  }

  fn write_line(&mut self, line: &[u8]) -> Result<(), Error> {
    // Avoids adding a leading newline to the output file.
    if !self.is_at_start_of_file {
      self.output.write_all(b"\n")?;
    }
    self.is_at_start_of_file = false;
    self.output.write_all(line)?;
    Ok(())
  }

  fn process_hunk<'p>(&mut self, hunk: &Hunk<'p>) -> Result<(), Error> {
    // This buffer stores lines from the source that are part of a potential match.
    // By keeping it local to this function, we improve encapsulation.
    let mut prospective_match_buffer = Vec::new();
    let get_context_iter = || {
      hunk
        .lines
        .iter()
        .filter(|l| matches!(l.kind, LineKind::Context | LineKind::Deletion))
    };

    // Advance to the target line before starting detailed matching.
    let target_line = hunk.old_line.saturating_sub(1);
    while self.current_source_line < target_line {
      if let Some(line) = self.source.next() {
        self.write_line(line)?;
        self.current_source_line += 1;
      } else {
        // If the source ends before we reach the hunk's target line,
        // and the hunk expected to find content, it's an error.
        if hunk.old_span > 0 {
          return Err(Error {
            line: Some(hunk.patch_line_num),
            kind: ErrorKind::CouldNotApplyHunk,
          });
        }
        break;
      }
    }

    // If a hunk has `old_span == 0`, it's a pure addition.
    // We just need to write the new lines.
    if hunk.old_span == 0 {
      for line in &hunk.lines {
        if line.is_addition() {
          self.write_line(line.text)?;
        }
      }
      return Ok(());
    }

    loop {
      let source_line = if let Some(line) = self.source.next() {
        line
      } else {
        // If we run out of source lines while trying to find a match, the hunk cannot be applied.
        return Err(Error {
          line: Some(hunk.patch_line_num),
          kind: ErrorKind::CouldNotApplyHunk,
        });
      };
      self.current_source_line += 1;

      let mut hunk_lines_iter = get_context_iter();

      let first_hunk_line = if let Some(line) = hunk_lines_iter.next() {
        line
      } else {
        // A hunk that is not a pure addition must have at least one context or deletion line.
        return Err(Error {
          line: Some(hunk.patch_line_num),
          kind: ErrorKind::CouldNotApplyHunk,
        });
      };

      if first_hunk_line.text != source_line {
        self.write_line(source_line)?;
        continue;
      }

      // We've found a potential match for the first line of the hunk.
      prospective_match_buffer.clear();
      prospective_match_buffer.push(source_line);

      let mut source_clone = self.source.clone();
      // Now, we check if the rest of the hunk's context/deletion lines match.
      for hunk_line in hunk_lines_iter {
        if let Some(next_source_line) = source_clone.next() {
          if next_source_line != hunk_line.text {
            // A mismatch was found inside a hunk that had started to match.
            // This is a definitive error.
            return Err(Error {
              line: Some(hunk_line.line_num),
              kind: ErrorKind::CouldNotApplyHunk,
            });
          }
          prospective_match_buffer.push(next_source_line);
        } else {
          // Reached end of source file while matching. This is a definitive error.
          return Err(Error {
            line: Some(hunk_line.line_num),
            kind: ErrorKind::CouldNotApplyHunk,
          });
        }
      }

      // If we're here, the hunk matched successfully.
      self.source = source_clone;
      self.current_source_line += hunk.old_span - 1;

      let mut matched_source_lines_index = 0;
      for line in &hunk.lines {
        match line.kind {
          LineKind::Addition => self.write_line(line.text)?,
          LineKind::Deletion => {
            matched_source_lines_index += 1;
          }
          LineKind::Context => {
            self.write_line(
              prospective_match_buffer[matched_source_lines_index],
            )?;
            matched_source_lines_index += 1;
          }
        }
      }
      return Ok(());
    }
  }

  fn process(mut self, patch: &Patch<'_>) -> Result<(), Error> {
    for hunk in &patch.hunks {
      self.process_hunk(hunk)?;
    }

    // After all hunks are processed, write any remaining lines from the source.
    while let Some(line) = self.source.next() {
      self.write_line(line)?;
    }

    // Standard git diff behavior is to ensure a final newline unless
    // explicitly told not to. This ensures compatibility.
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
  // If there are no changes (no hunks and no copy operation), we can
  // perform a fast-path by just writing the original source to the output.
  if patch.hunks.is_empty() && patch.copy_to.is_none() {
    output.write_all(source)?;
    return Ok(());
  }
  Applier::new(output, source).process(patch)
}

fn ignore_not_found(res: io::Result<()>) -> io::Result<()> {
  match res {
    // In many cases (e.g., deleting an already-deleted file), a "NotFound"
    // error is not a failure. This function suppresses it.
    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
    res => res,
  }
}

fn handle_file_deletion(
  fs: &mut impl FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  let source = fs.read(source_path).ok();
  let source_slice = source.as_deref().unwrap_or(&[]);
  // Before deleting, we "dry run" the patch against the source content
  // to ensure it would apply cleanly. This prevents accidental data loss
  // if the source file has changed unexpectedly. We write to a sink (null output).
  apply(&mut io::sink(), patch, source_slice)?;

  ignore_not_found(fs.remove_file(source_path))?;
  Ok(())
}

fn handle_metadata_change(
  fs: &mut impl FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  if patch.rename_to.is_some() {
    fs.rename(source_path, patch.new_file)?;
  } else if patch.copy_to.is_some() {
    fs.copy(source_path, patch.new_file)?;
  }
  Ok(())
}

fn handle_content_change(
  fs: &mut impl FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  let mut writer = fs.write(patch.new_file)?;
  {
    // For new files, there's no source. For existing files, we read them.
    // This handles both cases cleanly.
    let source = if patch.old_file == b"/dev/null" {
      None
    } else {
      fs.read(source_path).ok()
    };
    let source_slice = source.as_deref().unwrap_or(&[]);
    apply(&mut writer, patch, source_slice)?;
  }
  // The `AtomicWriter` ensures that the file is only moved to its final
  // destination after all content has been successfully written to a temp file.
  writer.commit()?;

  // If this was a rename, we need to clean up the original file after
  // creating the new one.
  if patch.rename_to.is_some() && source_path != patch.new_file {
    ignore_not_found(fs.remove_file(source_path))?;
  }
  Ok(())
}

fn patch_file_worker(
  fs: &mut impl FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  if patch.binary {
    return Err(Error {
      line: None,
      kind: ErrorKind::BinaryFilesNotSupported,
    });
  }

  // By using a `match` expression, we make the dispatch logic more explicit and idiomatic,
  // clearly distinguishing between file deletion, metadata changes, and content updates.
  match (patch.new_file, patch.hunks.is_empty()) {
    // A patch with a `/dev/null` new file signifies a deletion.
    (b"/dev/null", _) => handle_file_deletion(fs, patch)?,
    // A patch with no hunks indicates a metadata-only change (e.g., rename or copy).
    (_, true) => handle_metadata_change(fs, patch)?,
    // Otherwise, the patch involves content changes.
    (_, false) => handle_content_change(fs, patch)?,
  }

  // After content/metadata changes, apply any permission changes.
  if patch.new_file != b"/dev/null" {
    if let Some(mode) = patch.new_mode.or(patch.index_mode) {
      fs.set_permissions(patch.new_file, mode)?;
    }
  }

  Ok(())
}

pub fn patch_file(
  fs: &mut impl FileSystem,
  patch: Patch<'_>,
  reverse: bool,
) -> Result<(), Error> {
  // Applying a patch in reverse is as simple as inverting it first.
  // This avoids duplicating the main patching logic.
  if reverse {
    let inverted_patch = patch.invert();
    patch_file_worker(fs, &inverted_patch)
  } else {
    patch_file_worker(fs, &patch)
  }
}
