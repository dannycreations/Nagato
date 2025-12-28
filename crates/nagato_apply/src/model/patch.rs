use std::mem;

use crate::{BinaryFragment, Hunk};

/// Represents a single patch, which can contain multiple hunks.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct Patch<'a> {
  /// The index mode.
  pub index_mode: Option<u32>,
  /// The SHA1 hash of the old file.
  pub old_hash: Option<&'a [u8]>,
  /// The SHA1 hash of the new file.
  pub new_hash: Option<&'a [u8]>,
  /// The path to the old file.
  pub old_file: &'a [u8],
  /// The path to the new file.
  pub new_file: &'a [u8],
  /// The hunks in the patch.
  pub hunks: Vec<Hunk<'a>>,
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
  /// The deleted file mode.
  pub deleted_mode: Option<u32>,
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
  pub binary_fragments: Vec<BinaryFragment<'a>>,
}

impl<'a> Patch<'a> {
  /// Returns the source file for the patch, considering copy operations.
  pub fn source_file(&self) -> &'a [u8] {
    self.copy_from.unwrap_or(self.old_file)
  }

  /// Returns true if this patch creates a new file.
  pub fn is_creation(&self) -> bool {
    self.old_file == b"/dev/null"
  }

  /// Returns true if this patch deletes an existing file.
  pub fn is_deletion(&self) -> bool {
    self.new_file == b"/dev/null"
  }

  /// Returns true if this patch contains content changes (hunks or binary fragments).
  pub fn has_content_changes(&self) -> bool {
    !self.hunks.is_empty() || !self.binary_fragments.is_empty()
  }

  /// Invert the patch for reverse application.
  pub fn invert(mut self) -> Self {
    let is_creation = self.is_creation();
    let is_deletion = self.is_deletion();

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
