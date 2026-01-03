/// Represents a single token from a diff file.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind<'a> {
  /// The header of a file diff, containing the old and new file paths.
  FileHeader {
    old_file: &'a [u8],
    new_file: &'a [u8],
  },
  /// The index line, containing the hashes of the old and new files.
  Index {
    old_hash: &'a [u8],
    new_hash: &'a [u8],
    mode: Option<&'a [u8]>,
  },
  /// The old file path (`---`).
  OldFile(&'a [u8]),
  /// The new file path (`+++`).
  NewFile(&'a [u8]),
  /// The header of a hunk, containing the line numbers and spans.
  HunkHeader {
    old_range: &'a [u8],
    new_range: &'a [u8],
    label: Option<&'a [u8]>,
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
  NewFileMode(&'a [u8]),
  /// The old file mode.
  OldFileMode(&'a [u8]),
  /// The deleted file mode.
  DeletedFileMode(&'a [u8]),
  /// The similarity index in a rename or copy operation.
  Similarity(&'a [u8]),
  /// The dissimilarity index in a rename or copy operation.
  Dissimilarity(&'a [u8]),
  /// Indicates that two binary files are different.
  Binary {
    old_file: &'a [u8],
    new_file: &'a [u8],
  },
  /// The header of a git binary patch.
  GitBinaryPatchHeader,
  /// The type and size of a binary patch fragment.
  BinaryPatchType { kind: &'a [u8], size: &'a [u8] },
  /// A line of binary data.
  BinaryData(&'a [u8]),
  /// A label for the following hunk.
  Label(&'a [u8]),
}
