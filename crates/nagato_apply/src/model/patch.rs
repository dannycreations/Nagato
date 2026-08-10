use std::{
  borrow::Cow,
  io::{Result as IoResult, Write},
  mem,
};

use nagato_core::{IsDevNull, LineWriter};

use crate::{BinaryFragment, Hunk, Line, LineKind};

#[derive(Debug, PartialEq, Default, Clone)]
pub struct Patch<'a> {
  pub old_hash: Option<&'a [u8]>,
  pub new_hash: Option<&'a [u8]>,
  pub old_file: Cow<'a, [u8]>,
  pub new_file: Cow<'a, [u8]>,
  pub hunks: Vec<Hunk<'a>>,
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
  pub binary_fragments: Vec<BinaryFragment>,
  pub lines: Vec<Line<'a>>,
  pub binary_lines: Vec<&'a [u8]>,
}

impl<'a> Patch<'a> {
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.old_file.is_empty() && self.new_file.is_empty()
  }

  #[inline]
  pub fn hunk_lines(&self, hunk: &Hunk<'a>) -> &[Line<'a>] {
    let start = hunk.lines_start as usize;
    let end = start + hunk.lines_len as usize;
    &self.lines[start..end]
  }

  #[inline]
  pub fn binary_fragment_data(&self, fragment: &BinaryFragment) -> &[&'a [u8]] {
    let start = fragment.data_start as usize;
    let end = start + fragment.data_len as usize;
    &self.binary_lines[start..end]
  }

  pub fn source_file(&self) -> &[u8] {
    self.copy_from.as_deref().unwrap_or(&self.old_file)
  }

  pub fn has_content_changes(&self) -> bool {
    !self.hunks.is_empty() || !self.binary_fragments.is_empty()
  }

  pub fn filename(&self) -> &[u8] {
    match !self.new_file.is_empty() && !self.new_file.is_dev_null() {
      true => &self.new_file,
      false => &self.old_file,
    }
  }

  pub fn invert(mut self) -> Self {
    mem::swap(&mut self.old_file, &mut self.new_file);
    mem::swap(&mut self.old_hash, &mut self.new_hash);
    mem::swap(&mut self.old_mode, &mut self.new_mode);
    mem::swap(&mut self.old_file_no_newline, &mut self.new_file_no_newline);
    mem::swap(&mut self.copy_from, &mut self.copy_to);
    mem::swap(&mut self.rename_from, &mut self.rename_to);

    self.lines.iter_mut().for_each(|line| line.invert());
    self.hunks.iter_mut().for_each(|hunk| hunk.invert());
    self
  }

  pub fn append(&mut self, mut other: Self) {
    let lines_offset = self.lines.len() as u32;
    for hunk in other.hunks.iter_mut() {
      hunk.lines_start += lines_offset;
    }
    self.lines.extend(other.lines);
    self.hunks.extend(other.hunks);

    let binary_offset = self.binary_lines.len() as u32;
    for frag in other.binary_fragments.iter_mut() {
      frag.data_start += binary_offset;
    }
    self.binary_lines.extend(other.binary_lines);
    self.binary_fragments.extend(other.binary_fragments);
  }

  pub fn write_to(&self, out: &mut impl Write) -> IoResult<()> {
    let mut writer = LineWriter::new(out);

    writer.write_bytes(b"file ")?;
    writer.write_bytes(self.filename())?;
    writer.write_newline()?;

    for (i, hunk) in self.hunks.iter().enumerate() {
      if i > 0 || hunk.label.is_none() {
        writer.write_newline()?;
      }

      if let Some(label) = hunk.label {
        writer.write_bytes(b"label ")?;
        writer.write_bytes(label)?;
        writer.write_newline()?;
        writer.write_newline()?;
      }

      let lines = self.hunk_lines(hunk);
      for line in lines {
        let prefix = match line.kind {
          LineKind::Addition => Some(b'+'),
          LineKind::Deletion => Some(b'-'),
          LineKind::Context => Some(b' '),
          LineKind::Gap => None,
        };
        if let Some(p) = prefix {
          writer.write_bytes(&[p])?;
        }
        writer.write_bytes(line.text)?;
        writer.write_newline()?;
      }
    }

    Ok(())
  }
}
