use bstr::ByteSlice;
use memchr::memmem;
use nagato_core::{parse_int, ErrorKind};

use crate::{lexer::LexerMode, Lexer, TokenKind};

impl<'a> Lexer<'a> {
  #[inline]
  pub fn tokenize_binary(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.is_empty() {
      return Ok(TokenKind::Gap);
    }

    // The fragment headers are checked first, which is safe because the space
    // in their keyword puts them out of reach of the base85 payload alphabet.
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

    // Payload lines make up the bulk of a binary patch, so the leading byte is
    // compared before each full prefix match.
    let first = line[0];
    if (first == b'd' && line.starts_with(b"diff --git"))
      || (first == b'-' && line.starts_with(b"--- "))
      || (first == b'+' && line.starts_with(b"+++ "))
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
          Ok(TokenKind::FileHeader(line[5..].trim()))
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
          Self::parse_mode_rest(&line[4..], TokenKind::NewFileMode)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'o' => {
        if line.starts_with(b"old ") {
          Self::parse_mode_rest(&line[4..], TokenKind::OldFileMode)
        } else {
          Err(ErrorKind::UnexpectedLine)
        }
      }
      b'r' | b'c' => self.parse_rename_copy_line(line),
      b's' => {
        if line.starts_with(b"similarity index ") {
          Self::parse_percentage_token(&line[17..], TokenKind::Similarity)
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
    let header = &line[3..];
    let (ranges, label) = match memmem::find(header, b" @@") {
      Some(idx) => (&header[..idx], Some(header[idx + 3..].trim_start())),
      None => (header, None),
    };

    let (old_range, new_range) = parse_ranges(ranges)?;
    Ok(TokenKind::HunkHeader {
      old_range,
      new_range,
      label: label.filter(|label| !label.is_empty()),
    })
  }

  #[inline]
  fn parse_git_header(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if line.starts_with(b"diff --git ") {
      return Ok(TokenKind::FileHeader(&line[11..]));
    }

    if line.starts_with(b"dissimilarity index ") {
      let rest = &line[20..];
      return Self::parse_percentage_token(rest, TokenKind::Dissimilarity);
    }

    if line.starts_with(b"deleted ") {
      let rest = &line[8..];
      return Self::parse_mode_rest(rest, TokenKind::DeletedFileMode);
    }

    Err(ErrorKind::UnexpectedLine)
  }

  #[inline]
  fn parse_index_line(
    &mut self,
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
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
    let rest = &line[13..];
    let rest = rest.strip_suffix(b" differ").unwrap_or(rest);

    // We store the raw line segment to avoid eager Cow allocation and lifetime issues.
    Ok(TokenKind::Binary(rest))
  }

  #[inline]
  fn parse_mode_rest(
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

fn parse_ranges(s: &[u8]) -> Result<(&[u8], &[u8]), ErrorKind> {
  let mut parts = s.fields();
  let old_range = parts
    .next()
    .and_then(|r| r.strip_prefix(b"-"))
    .ok_or(ErrorKind::MissingRange)?;
  let new_range = parts
    .next()
    .and_then(|r| r.strip_prefix(b"+"))
    .ok_or(ErrorKind::MissingRange)?;
  Ok((old_range, new_range))
}
