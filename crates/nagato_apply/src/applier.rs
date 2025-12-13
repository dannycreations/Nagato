use std::{
  io::{self, Write},
  mem,
};

use bstr::{ByteSlice, Lines};
use nagato_core::{
  error::{ApplyError, Error},
  fs::FileSystem,
};

use crate::{Hunk, Line, Patch};

impl<'a> Patch<'a> {
  pub fn invert(mut self) -> Self {
    // Swapping fields is a direct and efficient way to invert the patch's metadata.
    mem::swap(&mut self.old_file, &mut self.new_file);
    mem::swap(&mut self.rename_from, &mut self.rename_to);
    mem::swap(&mut self.copy_from, &mut self.copy_to);
    mem::swap(&mut self.old_file_no_newline, &mut self.new_file_no_newline);

    if self.old_file == b"/dev/null" {
      // This was a file creation, so the inverse is a deletion.
      self.new_mode = self.deleted_mode;
      self.old_mode = None;
      self.deleted_mode = None;
    } else if self.new_file == b"/dev/null" {
      // This was a file deletion, so the inverse is a creation.
      self.deleted_mode = self.new_mode;
      self.new_mode = None;
    } else {
      // This was a modification, so we swap the modes.
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
    // Addition becomes deletion and vice-versa. Context lines remain unchanged.
    // This is done in-place for efficiency.
    self.lines.iter_mut().for_each(|line| {
      *line = match mem::replace(line, Line::Context(&[])) {
        Line::Addition(s) => Line::Deletion(s),
        Line::Deletion(s) => Line::Addition(s),
        Line::Context(s) => Line::Context(s),
      };
    });
  }
}

struct Applier<'s, 'b, W: Write + ?Sized> {
  output: &'b mut W,
  source: Lines<'s>,
  is_at_start_of_file: bool,
  current_source_line: u32,
  // By reusing this buffer, we avoid reallocating it for every hunk,
  // which is more memory-efficient when processing patches with many hunks.
  prospective_match_buffer: Vec<&'s [u8]>,
}

impl<'s, 'b, W: Write + ?Sized> Applier<'s, 'b, W> {
  fn new(output: &'b mut W, source: &'s [u8]) -> Self {
    Self {
      output,
      source: source.lines(),
      is_at_start_of_file: true,
      current_source_line: 0,
      // Initialize the buffer once to be reused across all hunk applications.
      prospective_match_buffer: Vec::new(),
    }
  }

  fn write_line(&mut self, line: &[u8]) -> io::Result<()> {
    // Avoids adding a leading newline to the output file.
    if !self.is_at_start_of_file {
      self.output.write_all(b"\n")?;
    }
    self.is_at_start_of_file = false;
    self.output.write_all(line)
  }

  fn process_hunk<'p>(&mut self, hunk: &Hunk<'p>) -> Result<(), Error> {
    // This closure provides an iterator over the context and deletion lines of a hunk,
    // which are the lines that need to be matched against the source file.
    // Using an iterator directly instead of collecting into a `Vec` avoids an
    // allocation, improving memory efficiency.
    let get_context_iter = || {
      hunk.lines.iter().filter_map(|l| match l {
        Line::Context(s) | Line::Deletion(s) => Some(*s),
        _ => None,
      })
    };

    // If a hunk contains only additions, we can fast-path. We advance the source
    // to the correct line and then write all the new lines.
    if hunk.old_span == 0 {
      let target_line = hunk.old_line.saturating_sub(1);
      while self.current_source_line < target_line {
        if let Some(line) = self.source.next() {
          self.write_line(line)?;
          self.current_source_line += 1;
        } else {
          break;
        }
      }
      for line in &hunk.lines {
        if let Line::Addition(s) = line {
          self.write_line(s)?;
        }
      }
      return Ok(());
    }

    // The 'search loop is the core of the fuzzy patch algorithm. It scans the source
    // file line by line, looking for a block that matches the hunk's context.
    'search: loop {
      let source_line = if let Some(line) = self.source.next() {
        line
      } else {
        return Err(ApplyError::CouldNotApplyHunk.into());
      };
      self.current_source_line += 1;

      let mut hunk_lines_iter = get_context_iter();

      // This is the first check. If the current source line doesn't match the
      // first context line of the hunk, we write it out and continue searching.
      if hunk_lines_iter.next() != Some(source_line) {
        self.write_line(source_line)?;
        continue;
      }

      self.prospective_match_buffer.clear();
      self.prospective_match_buffer.push(source_line);

      // We clone the source iterator to "look ahead" without consuming the original.
      // If the match fails, we can revert to the original iterator's state.
      let mut source_clone = self.source.clone();
      for hunk_line in hunk_lines_iter {
        if let Some(next_source_line) = source_clone.next() {
          if next_source_line == hunk_line {
            self.prospective_match_buffer.push(next_source_line);
          } else {
            // Match failed. Write the first line of the failed attempt and
            // jump back to the 'search loop to restart the search from the next line.
            self.write_line(source_line)?;
            continue 'search;
          }
        } else {
          // Reached end of source file while trying to match context.
          self.write_line(source_line)?;
          return Err(ApplyError::CouldNotApplyHunk.into());
        }
      }

      // A full match was found. We commit to this by replacing the main source
      // iterator with the cloned one that has advanced past the matched context.
      self.source = source_clone;
      self.current_source_line += hunk.old_span - 1;

      // We now write out the matched lines, interspersed with the additions.
      // By using an index instead of an iterator over `prospective_match_buffer`,
      // we avoid cloning the buffer, which is a key performance optimization.
      // This prevents an unnecessary heap allocation and memory copy for every hunk.
      let mut matched_source_lines_index = 0;
      for line in &hunk.lines {
        match line {
          Line::Addition(s) => self.write_line(s)?,
          Line::Deletion(_) => {
            // Deletions mean we effectively "skip" a line from the original
            // source by simply advancing our index into the matched buffer.
            matched_source_lines_index += 1;
          }
          Line::Context(_) => {
            // For context lines, we write the corresponding line from our
            // buffer of matched lines from the source file.
            self.write_line(
              self.prospective_match_buffer[matched_source_lines_index],
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
    return Err(Error::Message("Binary files are not supported"));
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
