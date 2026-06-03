use bstr::ByteSlice;
use memchr::memmem;
use nagato_core::{parse_int, ErrorKind};

use crate::{
  lexer::{token::BinaryPaths, LexerMode},
  Lexer, TokenKind,
};

impl<'a> Lexer<'a> {
  #[inline]
  pub fn tokenize_binary(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.is_empty() {
      return Ok(TokenKind::Gap);
    }

    // Fast path for binary data lines which usually start with base85 chars.
    if line.len() > 8 {
      if line.starts_with(b"literal ") {
        return Ok(TokenKind::BinaryPatchType {
          kind: b"literal",
          size: &line[8..],
        });
      }
      if line.starts_with(b"delta ") {
        return Ok(TokenKind::BinaryPatchType {
          kind: b"delta",
          size: &line[6..],
        });
      }
    }

    let first = line[0];
    if first == b'd' && line.starts_with(b"diff --git")
      || first == b'-' && line.starts_with(b"--- ")
      || first == b'+' && line.starts_with(b"+++ ")
    {
      self.set_mode(LexerMode::Text);
      return self.tokenize_text(line);
    }

    Ok(TokenKind::BinaryData(line))
  }

  #[inline]
  pub fn tokenize_text(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.is_empty() {
      return Ok(TokenKind::Gap);
    }

    let first = line[0];
    match first {
      b'+' => {
        if line.starts_with(b"+++ ") {
          Ok(TokenKind::NewFile(&line[4..]))
        } else {
          Ok(TokenKind::Addition(&line[1..]))
        }
      }
      b'-' => {
        if line.starts_with(b"--- ") {
          Ok(TokenKind::OldFile(&line[4..]))
        } else {
          Ok(TokenKind::Deletion(&line[1..]))
        }
      }
      b' ' => Ok(TokenKind::Context(&line[1..])),
      b'@' => {
        if line.starts_with(b"@@ ") {
          self.parse_hunk_header(line)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'd' => {
        if line.starts_with(b"diff ")
          || line.starts_with(b"dissimilarity ")
          || line.starts_with(b"deleted ")
        {
          self.parse_git_header(line)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'f' => {
        if line.starts_with(b"file ") {
          Ok(TokenKind::FileHeader(BinaryPaths {
            old_file: line[5..].trim(),
            new_file: line[5..].trim(),
          }))
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'G' => {
        if line == b"GIT binary patch" {
          self.set_mode(LexerMode::Binary);
          Ok(TokenKind::GitBinaryPatchHeader)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'i' => {
        if line.starts_with(b"index ") {
          self.parse_index_line(line)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'l' => {
        if line.starts_with(b"label ") {
          Ok(TokenKind::Label(line[6..].trim_start()))
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'n' => {
        if line.starts_with(b"new ") {
          self.parse_mode_rest(&line[4..], TokenKind::NewFileMode)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'o' => {
        if line.starts_with(b"old ") {
          self.parse_mode_rest(&line[4..], TokenKind::OldFileMode)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'r' | b'c' => self.parse_rename_copy_line(line),
      b's' => {
        if line.starts_with(b"similarity index ") {
          self.parse_percentage_token(&line[17..], TokenKind::Similarity)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'B' => {
        if line.starts_with(b"Binary files ") {
          self.parse_binary_files_line(line)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'\\' => {
        if line == b"\\ No newline at end of file" {
          Ok(TokenKind::NoNewline)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      _ => Err(ErrorKind::UnexpectedLine),
    }
  }

  #[inline]
  fn parse_hunk_header(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    // Hunk headers are parsed by splitting the line into range and optional label components using byte-level patterns.
    let header = &line[3..];
    let Some(idx) = memmem::find(header, b" @@") else {
      let mut parts = header.fields();
      let old_range = parts
        .next()
        .and_then(|s| s.strip_prefix(b"-"))
        .ok_or(ErrorKind::MissingRange)?;
      let new_range = parts
        .next()
        .and_then(|s| s.strip_prefix(b"+"))
        .ok_or(ErrorKind::MissingRange)?;

      return Ok(TokenKind::HunkHeader {
        old_range,
        new_range,
        label: None,
      });
    };

    let content = &header[..idx];
    let label_part = &header[idx + 3..];
    let mut parts = content.fields();

    let old_range = parts
      .next()
      .and_then(|s| s.strip_prefix(b"-"))
      .ok_or(ErrorKind::MissingRange)?;
    let new_range = parts
      .next()
      .and_then(|s| s.strip_prefix(b"+"))
      .ok_or(ErrorKind::MissingRange)?;

    let label = label_part.trim_start();
    let label = (!label.is_empty()).then_some(label);

    Ok(TokenKind::HunkHeader {
      old_range,
      new_range,
      label,
    })
  }

  #[inline]
  fn parse_git_header(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.starts_with(b"diff --git ") {
      let rest = &line[11..];
      return Ok(TokenKind::FileHeader(BinaryPaths {
        old_file: rest,
        new_file: rest,
      }));
    }

    if line.starts_with(b"dissimilarity index ") {
      let rest = &line[20..];
      return self.parse_percentage_token(rest, TokenKind::Dissimilarity);
    }

    if line.starts_with(b"deleted ") {
      let rest = &line[8..];
      return self.parse_mode_rest(rest, TokenKind::DeletedFileMode);
    }

    Err(ErrorKind::UnexpectedLine)
  }

  #[inline]
  fn parse_index_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    // Index lines are processed by extracting the hash pair and optional mode from the space-delimited fields.
    let mut parts = line[6..].fields();
    let (old_hash, new_hash) = parts
      .next()
      .and_then(|s| s.split_once_str(b".."))
      .ok_or(ErrorKind::InvalidIndexHeader)?;
    let mode = parts.next();
    Ok(TokenKind::Index {
      old_hash,
      new_hash,
      mode,
    })
  }

  #[inline]
  fn parse_rename_copy_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.starts_with(b"rename from ") {
      return Ok(TokenKind::RenameFrom(&line[12..]));
    }
    if line.starts_with(b"rename to ") {
      return Ok(TokenKind::RenameTo(&line[10..]));
    }
    if line.starts_with(b"copy from ") {
      return Ok(TokenKind::CopyFrom(&line[10..]));
    }
    if line.starts_with(b"copy to ") {
      return Ok(TokenKind::CopyTo(&line[8..]));
    }

    Err(ErrorKind::UnexpectedLine)
  }

  #[inline]
  fn parse_binary_files_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    // Binary file markers are parsed by extracting the file paths from a standardized "Binary files ... differ" message using byte-level split operations.
    let rest = &line[13..];
    let rest = rest.strip_suffix(b" differ").unwrap_or(rest);

    // We store the raw line segment to avoid eager Cow allocation and lifetime issues.
    Ok(TokenKind::Binary(BinaryPaths {
      old_file: rest,
      new_file: rest,
    }))
  }

  #[inline]
  fn parse_mode_rest(
    &self,
    rest: &'a [u8],
    f: impl FnOnce(&'a [u8]) -> TokenKind<'a>,
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(mode) = rest.strip_prefix(b"file mode ") {
      return Ok(f(mode));
    }

    if let Some(mode) = rest.strip_prefix(b"mode ") {
      return Ok(f(mode));
    }

    Err(ErrorKind::InvalidFileMode)
  }

  #[inline]
  fn parse_percentage_token(
    &self,
    s: &[u8],
    f: impl FnOnce(u32) -> TokenKind<'a>,
  ) -> Result<TokenKind<'a>, ErrorKind> {
    let s = s.strip_suffix(b"%").ok_or(ErrorKind::InvalidPercentage)?;
    let (num, rest) =
      parse_int::<u32>(s, 10).ok_or(ErrorKind::InvalidPercentage)?;
    if rest.is_empty() {
      Ok(f(num))
    } else {
      Err(ErrorKind::InvalidPercentage)
    }
  }
}
