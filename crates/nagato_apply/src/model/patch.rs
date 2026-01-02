use std::{
  io::{Result as IoResult, Write},
  mem,
};

use nagato_core::IsDevNull;

use crate::{BinaryFragment, Hunk, LineKind};

/// Represents a single patch, which can contain multiple hunks.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct Patch<'a> {
  /// The SHA1 hash of the old file.
  pub old_hash: Option<&'a [u8]>,
  /// The SHA1 hash of the new file.
  pub new_hash: Option<&'a [u8]>,
  /// The path to the old file.
  pub old_file: &'a [u8],
  /// The path to the new file.
  pub new_file: &'a [u8],
  /// The hunks in the patch.
  pub hunks: Box<[Hunk<'a>]>,
  /// The source file in a copy operation.
  pub copy_from: Option<&'a [u8]>,
  /// The destination file in a copy operation.
  pub copy_to: Option<&'a [u8]>,
  /// The old file name in a rename operation.
  pub rename_from: Option<&'a [u8]>,
  /// The new file name in a rename operation.
  pub rename_to: Option<&'a [u8]>,
  /// The new file mode.
  pub new_mode: Option<u32>,
  /// The old file mode.
  pub old_mode: Option<u32>,
  /// The similarity index in a rename or copy operation.
  pub similarity: Option<u32>,
  /// The dissimilarity index in a rename or copy operation.
  pub dissimilarity: Option<u32>,
  /// Indicates whether the patch is for a binary file.
  pub binary: bool,
  /// Indicates that the old file has no newline at the end.
  pub old_file_no_newline: bool,
  /// Indicates that the new file has no newline at the end.
  pub new_file_no_newline: bool,
  /// Binary patch fragments.
  pub binary_fragments: Box<[BinaryFragment<'a>]>,
}

impl<'a> Patch<'a> {
  /// Returns the source file for the patch, considering copy operations.
  pub fn source_file(&self) -> &'a [u8] {
    self.copy_from.unwrap_or(self.old_file)
  }

  /// Returns true if this patch contains content changes (hunks or binary fragments).
  pub fn has_content_changes(&self) -> bool {
    !self.hunks.is_empty() || !self.binary_fragments.is_empty()
  }

  /// Returns the target filename for the patch.
  pub fn filename(&self) -> &'a [u8] {
    if !self.new_file.is_empty() && !self.new_file.is_dev_null() {
      self.new_file
    } else {
      self.old_file
    }
  }

  /// Invert the patch for reverse application.
  pub fn invert(mut self) -> Self {
    mem::swap(&mut self.old_file, &mut self.new_file);
    mem::swap(&mut self.rename_from, &mut self.rename_to);
    mem::swap(&mut self.copy_from, &mut self.copy_to);
    mem::swap(&mut self.old_file_no_newline, &mut self.new_file_no_newline);
    mem::swap(&mut self.old_hash, &mut self.new_hash);
    mem::swap(&mut self.old_mode, &mut self.new_mode);

    for hunk in self.hunks.iter_mut() {
      hunk.invert();
    }
    self
  }

  /// Serialize the patch into the Nagato "trimmed" format.
  pub fn to_bytes(&self, out: &mut impl Write) -> IoResult<()> {
    out.write_all(b"file ")?;
    out.write_all(self.filename())?;
    out.write_all(b"\n")?;

    for (i, hunk) in self.hunks.iter().enumerate() {
      // If it's the first hunk and no label, add extra newline after file header
      // Otherwise, add newline between hunks
      if i == 0 {
        if hunk.label.is_none() {
          out.write_all(b"\n")?;
        }
      } else {
        out.write_all(b"\n")?;
      }

      // Replace hunk header with `label` if label exists
      if let Some(label) = hunk.label {
        out.write_all(b"label ")?;
        out.write_all(label)?;
        out.write_all(b"\n\n")?;
      }

      for line in &hunk.lines {
        let prefix = match line.kind {
          LineKind::Addition => b'+',
          LineKind::Deletion => b'-',
          LineKind::Context => b' ',
        };
        out.write_all(&[prefix])?;
        out.write_all(line.text)?;
        out.write_all(b"\n")?;
      }
    }

    Ok(())
  }
}
