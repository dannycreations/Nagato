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
    if let Some(size) = line.strip_prefix(b"literal ") {
      return Ok(TokenKind::BinaryPatchType {
        kind: b"literal",
        size,
      });
    }
    if let Some(size) = line.strip_prefix(b"delta ") {
      return Ok(TokenKind::BinaryPatchType {
        kind: b"delta",
        size,
      });
    }

    // Payload lines make up the bulk of a binary patch. The space in each
    // keyword keeps these prefixes out of the payload alphabet, so a payload
    // line only pays one failed leading byte per prefix.
    if line.starts_with(b"diff --git")
      || line.starts_with(b"--- ")
      || line.starts_with(b"+++ ")
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

    // The leading byte narrows the candidates down before a prefix is
    // compared. A line whose keyword does not match in full is not a line of
    // a patch, whatever it starts with.
    match line[0] {
      b'+' => match line.strip_prefix(b"+++ ") {
        Some(path) => Ok(TokenKind::NewFile(path)),
        None => Ok(TokenKind::Addition(&line[1..])),
      },
      b'-' => match line.strip_prefix(b"--- ") {
        Some(path) => Ok(TokenKind::OldFile(path)),
        None => Ok(TokenKind::Deletion(&line[1..])),
      },
      b' ' => Ok(TokenKind::Context(&line[1..])),
      b'@' => match line.strip_prefix(b"@@ ") {
        Some(header) => Self::parse_hunk_header(header),
        None => Err(ErrorKind::UnexpectedLine),
      },
      b'd' => Self::parse_git_header(line),
      b'f' => match line.strip_prefix(b"file ") {
        Some(path) => Ok(TokenKind::FileHeader(path.trim())),
        None => Err(ErrorKind::UnexpectedLine),
      },
      b'G' if line == b"GIT binary patch" => {
        self.set_mode(LexerMode::Binary);
        Ok(TokenKind::GitBinaryPatchHeader)
      }
      b'i' => match line.strip_prefix(b"index ") {
        Some(hashes) => Self::parse_index_line(hashes),
        None => Err(ErrorKind::UnexpectedLine),
      },
      b'l' => match line.strip_prefix(b"label ") {
        Some(label) => Ok(TokenKind::Label(label.trim_start())),
        None => Err(ErrorKind::UnexpectedLine),
      },
      b'n' => match line.strip_prefix(b"new ") {
        Some(rest) => Self::parse_mode_rest(rest, TokenKind::NewFileMode),
        None => Err(ErrorKind::UnexpectedLine),
      },
      b'o' => match line.strip_prefix(b"old ") {
        Some(rest) => Self::parse_mode_rest(rest, TokenKind::OldFileMode),
        None => Err(ErrorKind::UnexpectedLine),
      },
      b'r' | b'c' => Self::parse_rename_copy_line(line),
      b's' => match line.strip_prefix(b"similarity index ") {
        Some(percent) => {
          Self::parse_percentage_token(percent, TokenKind::Similarity)
        }
        None => Err(ErrorKind::UnexpectedLine),
      },
      // The raw segment is kept so that the parser splits both paths with the
      // same quoting rules a `diff --git` header goes through.
      b'B' => match line.strip_prefix(b"Binary files ") {
        Some(rest) => Ok(TokenKind::Binary(
          rest.strip_suffix(b" differ").unwrap_or(rest),
        )),
        None => Err(ErrorKind::UnexpectedLine),
      },
      b'\\' if line == b"\\ No newline at end of file" => {
        Ok(TokenKind::NoNewline)
      }
      _ => Err(ErrorKind::UnexpectedLine),
    }
  }

  #[inline]
  fn parse_hunk_header(header: &'a [u8]) -> Result<TokenKind<'a>, ErrorKind> {
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
  fn parse_git_header(line: &'a [u8]) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(paths) = line.strip_prefix(b"diff --git ") {
      return Ok(TokenKind::FileHeader(paths));
    }

    if let Some(percent) = line.strip_prefix(b"dissimilarity index ") {
      return Self::parse_percentage_token(percent, TokenKind::Dissimilarity);
    }

    if let Some(rest) = line.strip_prefix(b"deleted ") {
      return Self::parse_mode_rest(rest, TokenKind::DeletedFileMode);
    }

    Err(ErrorKind::UnexpectedLine)
  }

  #[inline]
  fn parse_index_line(hashes: &'a [u8]) -> Result<TokenKind<'a>, ErrorKind> {
    let mut parts = hashes.fields();
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
    line: &'a [u8],
  ) -> Result<TokenKind<'a>, ErrorKind> {
    if let Some(path) = line.strip_prefix(b"rename from ") {
      return Ok(TokenKind::RenameFrom(path));
    }
    if let Some(path) = line.strip_prefix(b"rename to ") {
      return Ok(TokenKind::RenameTo(path));
    }
    if let Some(path) = line.strip_prefix(b"copy from ") {
      return Ok(TokenKind::CopyFrom(path));
    }
    if let Some(path) = line.strip_prefix(b"copy to ") {
      return Ok(TokenKind::CopyTo(path));
    }

    Err(ErrorKind::UnexpectedLine)
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
