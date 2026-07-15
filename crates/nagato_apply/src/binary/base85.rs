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

use flate2::read::ZlibDecoder;
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

const DECODE_LEN_MAP: [u8; 256] = {
  let mut map = [0xFFu8; 256];
  let mut i = 0u8;
  while i < 26 {
    map[(b'A' + i) as usize] = i + 1;
    map[(b'a' + i) as usize] = i + 27;
    i += 1;
  }
  map
};

#[inline]
fn decode_len_char(c: u8) -> Option<usize> {
  let res = DECODE_LEN_MAP[c as usize];
  if res == 0xFF {
    None
  } else {
    Some(res as usize)
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

      let Some(line) = self.lines.next() else {
        return Ok(0);
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
      for chunk in data.as_chunks::<5>().0 {
        let d0 = DECODE_MAP[chunk[0] as usize];
        let d1 = DECODE_MAP[chunk[1] as usize];
        let d2 = DECODE_MAP[chunk[2] as usize];
        let d3 = DECODE_MAP[chunk[3] as usize];
        let d4 = DECODE_MAP[chunk[4] as usize];

        if (d0 | d1 | d2 | d3 | d4) == 0xFF {
          return Err(IoError::new(
            IoErrorKind::InvalidData,
            InvalidBinaryLineError,
          ));
        }

        let val = (d0 as u32) * 52_200_625
          + (d1 as u32) * 614_125
          + (d2 as u32) * 7_225
          + (d3 as u32) * 85
          + (d4 as u32);

        self.buffer[self.buf_len..self.buf_len + 4]
          .copy_from_slice(&val.to_be_bytes());
        self.buf_len += 4;
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
  let mut decoder = ZlibDecoder::new(Base85Reader::new(lines));
  io_copy(&mut decoder, writer).map_err(|e| {
    let mut is_invalid_line = false;
    if let Some(r) = e.get_ref() {
      if r.is::<InvalidBinaryLineError>() {
        is_invalid_line = true;
      }
    }

    if is_invalid_line {
      return Error::from(ErrorKind::InvalidBinaryFilesLine);
    }
    Error::from(e)
  })?;
  Ok(())
}
