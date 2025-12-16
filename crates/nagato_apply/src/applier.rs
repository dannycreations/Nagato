use std::{
  io::{self, sink, Write},
  mem,
};

use bstr::{ByteSlice, Lines};
use memmap2::Mmap;
use nagato_core::{
  error::{Error, ErrorKind},
  fs::FileSystem,
};

use crate::{Hunk, Line, LineKind, Patch};

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

  // This new private method is responsible for advancing the source file to the
  // line where the hunk is expected to apply. This simplifies the main `process_hunk`
  // function by isolating this preparatory step.
  fn advance_to_hunk(&mut self, hunk: &Hunk) -> Result<(), Error> {
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
    Ok(())
  }

  // I am extracting the hunk matching logic into its own function.
  // This simplifies the `find_and_apply_hunk` function by separating the
  // "finding" from the "applying", improving readability and maintainability.
  fn find_hunk_match<'p>(
    &mut self,
    hunk: &Hunk<'p>,
    lines_to_match: impl Iterator<Item = (usize, &'p Line<'p>)> + Clone,
    first_line_to_match: &Line,
  ) -> Result<(), Error> {
    // This is the main search loop. It consumes lines from the source until a
    // match is found or the source is exhausted.
    loop {
      // 1. Get the next line from the source.
      let source_line = if let Some(line) = self.source.next() {
        line
      } else {
        // We've reached the end of the source file without finding a match.
        return Err(Error {
          line: Some(hunk.patch_line_num),
          kind: ErrorKind::CouldNotApplyHunk,
        });
      };
      self.current_source_line += 1;
      // 2. Fast-path check: Does this source line match the FIRST line of the hunk?
      if source_line != first_line_to_match.text {
        // No match. Write this source line to the output and continue the search.
        self.write_line(source_line)?;
        continue;
      }
      // 3. Potential match found. Now we must verify the REST of the hunk.
      // We clone the source iterator to perform a speculative match. If it fails,
      // the original `self.source` iterator is unaffected.
      let mut source_clone = self.source.clone();
      // The `lines_to_match` iterator is cloned here for the speculative match.
      // Since it was already advanced by one, it now represents the rest of the
      // lines that need to be matched.
      for (offset, hunk_line) in lines_to_match.clone() {
        if let Some(next_source_line) = source_clone.next() {
          if next_source_line != hunk_line.text {
            // HARD FAILURE: A partial match that then fails is a fatal error
            // for this hunk, as per `git apply` behavior.
            return Err(Error {
              // Calculate the error line number dynamically.
              // hunk.patch_line_num is the line number of the hunk header.
              // We add 1 for the header itself, plus the offset of the line in the hunk.
              line: Some(hunk.patch_line_num + 1 + offset as u32),
              kind: ErrorKind::CouldNotApplyHunk,
            });
          }
        } else {
          // End of source during a speculative match. This is also a hard failure.
          return Err(Error {
            line: Some(hunk.patch_line_num + 1 + offset as u32),
            kind: ErrorKind::CouldNotApplyHunk,
          });
        }
      }
      // 4. Full match confirmed!
      // We commit the speculative read by updating the main source iterator.
      self.source = source_clone;
      return Ok(());
    }
  }

  // This function is the heart of the patch application logic. It finds the
  // location in the source file where a hunk should be applied and performs the
  // changes. It implements a search that aborts with an error if a partial match
  // is found, which mimics the behavior of `git apply`.
  fn find_and_apply_hunk<'p>(&mut self, hunk: &Hunk<'p>) -> Result<(), Error> {
    // By creating an iterator and immediately taking the first item, we avoid
    // allocating a `Vec` for all matching lines. This is a small but meaningful
    // optimization that reduces heap allocation in a hot loop.
    let mut lines_to_match = hunk
      .lines
      .iter()
      .enumerate()
      .filter(|(_, l)| !matches!(l.kind, LineKind::Addition));
    let first_line_to_match = if let Some((_, line)) = lines_to_match.next() {
      line
    } else {
      // This is a pure-addition hunk. This case should be handled earlier in `process_hunk`,
      // but we return Ok(()) here as a safeguard.
      return Ok(());
    };

    // The hunk matching logic is now in its own function, `find_hunk_match`.
    // This makes the code flow easier to follow.
    self.find_hunk_match(hunk, lines_to_match, first_line_to_match)?;

    // With the match found, we can now apply the changes.
    self.current_source_line += hunk.old_span - 1;

    for line in &hunk.lines {
      match line.kind {
        LineKind::Addition => self.write_line(line.text)?,
        LineKind::Deletion => {
          // For deletions, we simply do nothing. The source lines were already
          // "consumed" by `find_hunk_match` (by updating `self.source`).
        }
        LineKind::Context => {
          // For context lines, we write the text from the patch line.
          // Since we verified exact equality in `find_hunk_match`, this is safe.
          // This avoids needing to buffer the matched source lines.
          self.write_line(line.text)?;
        }
      }
    }

    Ok(())
  }

  // The `process_hunk` function is now much simpler. It delegates the work to the
  // new `advance_to_hunk` and `find_and_apply_hunk` methods, making the overall
  // logic easier to follow. It also handles the special case of pure-addition hunks.
  fn process_hunk<'p>(&mut self, hunk: &Hunk<'p>) -> Result<(), Error> {
    self.advance_to_hunk(hunk)?;

    // If a hunk has `old_span == 0`, it's a pure addition.
    // We just need to write the new lines.
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

fn ignore_not_found(res: Result<(), Error>) -> Result<(), Error> {
  match res {
    Err(Error {
      kind: ErrorKind::Io(e),
      ..
    }) if e.kind() == io::ErrorKind::NotFound => Ok(()),
    res => res,
  }
}

// This new helper function centralizes the logic for reading a source file,
// treating a `NotFound` error as an empty file. This avoids code duplication
// in `handle_file_deletion` and `handle_content_change`.
fn read_source_or_empty(
  fs: &impl FileSystem,
  path: &[u8],
) -> Result<Option<Mmap>, Error> {
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
  fs: &mut impl FileSystem,
  patch: &Patch<'_>,
) -> Result<(), Error> {
  let source_path = patch.source_file();
  let source = read_source_or_empty(fs, source_path)?;
  let source_slice = source.as_deref().unwrap_or(&[]);
  // Before deleting, we "dry run" the patch against the source content
  // to ensure it would apply cleanly. This prevents accidental data loss
  // if the source file has changed unexpectedly. We write to a sink (null output).
  apply(&mut sink(), patch, source_slice)?;

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
  } else if patch.old_file == b"/dev/null" {
    // This case handles the creation of a new, empty file, which is common
    // for binary files where the patch does not contain content.
    fs.write(patch.new_file)?.commit()?;
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
    // The logic to read the source file is now encapsulated in `read_source_or_empty`,
    // simplifying this function and removing duplicated code.
    let source = read_source_or_empty(fs, source_path)?;
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
  // Patches that are marked as binary but also contain hunks are not supported.
  // This is because the "Binary files differ" format doesn't provide enough
  // information to apply content changes.
  if patch.binary && !patch.hunks.is_empty() {
    return Err(Error {
      line: None,
      kind: ErrorKind::UnsupportedBinaryPatch,
    });
  }

  // By using a `match` expression, we make the dispatch logic more explicit and idiomatic,
  // clearly distinguishing between file deletion, metadata changes, and content updates.
  match (patch.new_file, patch.hunks.is_empty()) {
    // A patch with a `/dev/null` new file signifies a deletion.
    (b"/dev/null", _) => handle_file_deletion(fs, patch)?,
    // A patch with no hunks indicates a metadata-only change (e.g., rename or copy).
    // This also applies to binary files that are created empty.
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
