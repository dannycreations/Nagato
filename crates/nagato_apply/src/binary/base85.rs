use std::{
  cmp,
  error::Error as StdError,
  fmt::{Display, Formatter, Result as FmtResult},
  io::{
    self, Error as IoError, ErrorKind as IoErrorKind, Read, Result as IoResult,
    Write,
  },
  slice::Iter,
};

use nagato_core::{Error, ErrorKind};

/// Git's base85 alphabet
const ENCODE_MAP: &[u8; 85] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";

fn decode_char(c: u8) -> Option<u8> {
  ENCODE_MAP.iter().position(|&x| x == c).map(|x| x as u8)
}

fn decode_len_char(c: u8) -> Option<usize> {
  if c.is_ascii_uppercase() {
    Some((c - b'A' + 1) as usize)
  } else if c.is_ascii_lowercase() {
    Some((c - b'a' + 27) as usize)
  } else {
    None
  }
}

#[derive(Debug)]
pub struct InvalidBinaryLineError;

impl Display for InvalidBinaryLineError {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    write!(f, "Invalid binary files line")
  }
}

impl StdError for InvalidBinaryLineError {}

pub struct Base85Reader<'a> {
  lines: Iter<'a, &'a [u8]>,
  buffer: [u8; 52], // Git binary lines are at most 52 bytes decoded (Z line is 52)
  buf_len: usize,
  pos: usize,
}

impl<'a> Base85Reader<'a> {
  pub fn new(lines: &'a [&'a [u8]]) -> Self {
    Self {
      lines: lines.iter(),
      buffer: [0u8; 52],
      buf_len: 0,
      pos: 0,
    }
  }
}

impl<'a> Read for Base85Reader<'a> {
  fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
    loop {
      if self.pos < self.buf_len {
        let len = cmp::min(buf.len(), self.buf_len - self.pos);
        buf[..len].copy_from_slice(&self.buffer[self.pos..self.pos + len]);
        self.pos += len;
        return Ok(len);
      }

      let line = match self.lines.next() {
        Some(l) => l,
        None => return Ok(0),
      };

      if line.is_empty() {
        continue;
      }

      self.buf_len = 0;
      self.pos = 0;

      let len_char = line[0];
      let expected_len = decode_len_char(len_char).ok_or_else(|| {
        IoError::new(IoErrorKind::InvalidData, InvalidBinaryLineError)
      })?;

      let data = &line[1..];
      for chunk in data.chunks(5) {
        if chunk.len() < 5 {
          break;
        }

        let mut val: u32 = 0;
        for &c in chunk {
          val = val.checked_mul(85).ok_or_else(|| {
            IoError::new(IoErrorKind::InvalidData, InvalidBinaryLineError)
          })?;
          val = val
            .checked_add(decode_char(c).ok_or_else(|| {
              IoError::new(IoErrorKind::InvalidData, InvalidBinaryLineError)
            })? as u32)
            .ok_or_else(|| {
              IoError::new(IoErrorKind::InvalidData, InvalidBinaryLineError)
            })?;
        }

        if self.buf_len + 4 <= self.buffer.len() {
          self.buffer[self.buf_len] = (val >> 24) as u8;
          self.buffer[self.buf_len + 1] = (val >> 16) as u8;
          self.buffer[self.buf_len + 2] = (val >> 8) as u8;
          self.buffer[self.buf_len + 3] = val as u8;
          self.buf_len += 4;
        }
      }

      if self.buf_len > expected_len {
        self.buf_len = expected_len;
      }
    }
  }
}

pub fn decode_base85(
  lines: &[&[u8]],
  writer: &mut impl Write,
) -> Result<(), Error> {
  use flate2::read::ZlibDecoder;
  let mut decoder = ZlibDecoder::new(Base85Reader::new(lines));
  io::copy(&mut decoder, writer).map_err(|e| {
    if e
      .get_ref()
      .map(|r| r.is::<InvalidBinaryLineError>())
      .unwrap_or(false)
    {
      Error::new(ErrorKind::InvalidBinaryFilesLine)
    } else {
      Error::new(ErrorKind::Io(e.into()))
    }
  })?;
  Ok(())
}
