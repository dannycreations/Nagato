mod applier;
mod lexer;
mod parser;

pub use applier::{apply, patch_file};
pub use lexer::Lexer;
pub use parser::Parser;

/// Represents a single token from a diff file.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub enum Token<'a> {
  /// The header of a file diff, containing the old and new file paths.
  FileHeader {
    old_file: &'a [u8],
    new_file: &'a [u8],
  },
  /// The index line, containing the hashes of the old and new files.
  Index {
    old_hash: &'a str,
    new_hash: &'a str,
    mode: Option<u32>,
  },
  /// The old file path (`---`).
  OldFile(&'a [u8]),
  /// The new file path (`+++`).
  NewFile(&'a [u8]),
  /// The header of a hunk, containing the line numbers and spans.
  HunkHeader {
    old_line: u32,
    old_span: u32,
    new_line: u32,
    new_span: u32,
  },
  /// A line that was added to the file.
  Addition(&'a [u8]),
  /// A line that was deleted from the file.
  Deletion(&'a [u8]),
  /// A line that is part of the context.
  Context(&'a [u8]),
  /// Indicates that there is no newline at the end of the file.
  NoNewline,
  /// The source file in a copy operation.
  CopyFrom(&'a [u8]),
  /// The destination file in a copy operation.
  CopyTo(&'a [u8]),
  /// The old file name in a rename operation.
  RenameFrom(&'a [u8]),
  /// The new file name in a rename operation.
  RenameTo(&'a [u8]),
  /// The new file mode.
  NewFileMode(u32),
  /// The old file mode.
  OldFileMode(u32),
  /// The deleted file mode.
  DeletedFileMode(u32),
  /// The similarity index in a rename or copy operation.
  Similarity(u32),
  /// The dissimilarity index in a rename or copy operation.
  Dissimilarity(u32),
  /// Indicates that two binary files are different.
  Binary {
    old_file: &'a [u8],
    new_file: &'a [u8],
  },
}

/// Represents a single line in a hunk.
#[derive(Debug, Clone, PartialEq)]
pub enum Line<'a> {
  /// A line that was added.
  Addition(&'a [u8]),
  /// A line that was deleted.
  Deletion(&'a [u8]),
  /// A line that is part of the context.
  Context(&'a [u8]),
}

impl<'a> Line<'a> {
  pub fn is_addition(&self) -> bool {
    matches!(self, Line::Addition(_))
  }

  pub fn text(&self) -> &'a [u8] {
    // This was refactored from a `match` to a more direct destructuring,
    // as all variants of `Line` contain a single `&[u8]` element.
    // This is more concise and equally clear.
    let (Line::Addition(text) | Line::Deletion(text) | Line::Context(text)) =
      self;
    text
  }
}

/// Represents a single hunk in a patch.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct Hunk<'a> {
  /// The starting line number of the old file.
  pub old_line: u32,
  /// The number of lines in the old file.
  pub old_span: u32,
  /// The starting line number of the new file.
  pub new_line: u32,
  /// The number of lines in the new file.
  pub new_span: u32,
  /// The lines in the hunk.
  pub lines: Vec<Line<'a>>,
}

/// Represents a single patch, which can contain multiple hunks.
#[derive(Debug, PartialEq, Default, Clone)]
pub struct Patch<'a> {
  /// The index mode.
  pub index_mode: Option<u32>,
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
}

impl<'a> Patch<'a> {
  /// Returns the source file for the patch, considering copy operations.
  pub fn source_file(&self) -> &'a [u8] {
    self.copy_from.unwrap_or(self.old_file)
  }
}
