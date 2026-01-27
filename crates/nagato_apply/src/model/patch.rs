use std::{
  borrow::Cow,
  io::{Result as IoResult, Write},
  mem,
};

use nagato_core::{IsDevNull, LineWriter};

use crate::{BinaryFragment, Hunk, LineKind};

#[derive(Debug, PartialEq, Default, Clone)]
pub struct Patch<'a> {
  pub old_hash: Option<&'a [u8]>,
  pub new_hash: Option<&'a [u8]>,
  pub old_file: Cow<'a, [u8]>,
  pub new_file: Cow<'a, [u8]>,
  pub hunks: Box<[Hunk<'a>]>,
  pub copy_from: Option<Cow<'a, [u8]>>,
  pub copy_to: Option<Cow<'a, [u8]>>,
  pub rename_from: Option<Cow<'a, [u8]>>,
  pub rename_to: Option<Cow<'a, [u8]>>,
  pub new_mode: Option<u32>,
  pub old_mode: Option<u32>,
  pub similarity: Option<u32>,
  pub dissimilarity: Option<u32>,
  pub binary: bool,
  pub old_file_no_newline: bool,
  pub new_file_no_newline: bool,
  pub binary_fragments: Box<[BinaryFragment<'a>]>,
}

impl<'a> Patch<'a> {
  pub fn source_file(&self) -> &[u8] {
    self.copy_from.as_deref().unwrap_or(&self.old_file)
  }

  pub fn has_content_changes(&self) -> bool {
    !self.hunks.is_empty() || !self.binary_fragments.is_empty()
  }

  pub fn filename(&self) -> &[u8] {
    if !self.new_file.is_empty() && !self.new_file.is_dev_null() {
      &self.new_file
    } else {
      &self.old_file
    }
  }

  pub fn invert(mut self) -> Self {
    mem::swap(&mut self.old_file, &mut self.new_file);
    mem::swap(&mut self.old_hash, &mut self.new_hash);
    mem::swap(&mut self.old_mode, &mut self.new_mode);
    mem::swap(&mut self.old_file_no_newline, &mut self.new_file_no_newline);
    mem::swap(&mut self.copy_from, &mut self.copy_to);
    mem::swap(&mut self.rename_from, &mut self.rename_to);

    self.hunks.iter_mut().for_each(|hunk| hunk.invert());
    self
  }

  pub fn to_bytes(&self, out: &mut impl Write) -> IoResult<()> {
    let mut writer = LineWriter::new(out);

    writer.write_bytes(b"file ")?;
    writer.write_bytes(self.filename())?;
    writer.write_newline()?;

    self.hunks.iter().enumerate().try_for_each(|(i, hunk)| {
      // Hunk separation is maintained by ensuring a newline precedes every hunk except for the first one when it contains a label.
      if i > 0 || hunk.label.is_none() {
        writer.write_newline()?;
      }

      // Replace hunk header with `label` if label exists
      if let Some(label) = hunk.label {
        writer.write_bytes(b"label ")?;
        writer.write_bytes(label)?;
        writer.write_newline()?;
        writer.write_newline()?;
      }

      hunk.lines.iter().try_for_each(|line| {
        let prefix = match line.kind {
          LineKind::Addition => b'+',
          LineKind::Deletion => b'-',
          LineKind::Context => b' ',
        };
        writer.write_bytes(&[prefix])?;
        writer.write_bytes(line.text)?;
        writer.write_newline()
      })
    })?;

    Ok(())
  }
}
