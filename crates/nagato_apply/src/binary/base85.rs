use std::{
  cmp,
  error::Error as StdError,
  fmt::{Display, Formatter, Result as FmtResult},
  io::{
    copy as io_copy, Error as IoError, ErrorKind as IoErrorKind, Read,
    Result as IoResult, Write,
  },
  slice::Iter,
};

use nagato_core::{Error, ErrorKind};

// Git's base85 alphabet
const ENCODE_MAP: &[u8; 85] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";
// Maximum decoded length of a single git binary line (Z = 52 bytes)
const MAX_DECODED_LINE_LEN: usize = 52;

// Precomputed lookup table for base85 decoding to avoid linear searches.
// Values are stored as u8 where 0xFF indicates an invalid character.
const DECODE_MAP: [u8; 256] = {
  let mut map = [0xFFu8; 256];
  let mut i = 0;
  while i < 85 {
    map[ENCODE_MAP[i] as usize] = i as u8;
    i += 1;
  }
  map
};

fn decode_len_char(c: u8) -> Option<usize> {
  match c {
    b'A'..=b'Z' => Some((c - b'A' + 1) as usize),
    b'a'..=b'z' => Some((c - b'a' + 27) as usize),
    _ => None,
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
  buffer: [u8; MAX_DECODED_LINE_LEN],
  buf_len: usize,
  pos: usize,
}

impl<'a> Base85Reader<'a> {
  pub fn new(lines: &'a [&'a [u8]]) -> Self {
    Self {
      lines: lines.iter(),
      buffer: [0u8; MAX_DECODED_LINE_LEN],
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
      let mut i = 0;
      while i + 5 <= data.len() {
        let mut val = 0u64;
        for j in 0..5 {
          let decoded = DECODE_MAP[data[i + j] as usize];
          if decoded == 0xFF {
            return Err(IoError::new(
              IoErrorKind::InvalidData,
              InvalidBinaryLineError,
            ));
          }
          val = val * 85 + decoded as u64;
        }

        if val > u32::MAX as u64 {
          return Err(IoError::new(
            IoErrorKind::InvalidData,
            InvalidBinaryLineError,
          ));
        }

        self.buffer[self.buf_len..self.buf_len + 4]
          .copy_from_slice(&(val as u32).to_be_bytes());
        self.buf_len += 4;
        i += 5;
      }

      if self.buf_len > expected_len {
        self.buf_len = expected_len;
      }
    }
  }
}

pub fn decode_base85(
  lines: &[&[u8]],
  writer: &mut (impl Write + ?Sized),
) -> Result<(), Error> {
  use flate2::read::ZlibDecoder;
  let mut decoder = ZlibDecoder::new(Base85Reader::new(lines));
  io_copy(&mut decoder, writer).map_err(|e| {
    if e
      .get_ref()
      .map(|r| r.is::<InvalidBinaryLineError>())
      .unwrap_or(false)
    {
      Error::from(ErrorKind::InvalidBinaryFilesLine)
    } else {
      Error::from(e)
    }
  })?;
  Ok(())
}
