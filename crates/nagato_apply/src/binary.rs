use std::io::{self, Read, Write};

use flate2::read::ZlibDecoder;
use nagato_core::error::{Error, ErrorKind};

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
struct InvalidBinaryLineError;

impl std::fmt::Display for InvalidBinaryLineError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Invalid binary files line")
  }
}

impl std::error::Error for InvalidBinaryLineError {}

pub struct Base85Reader<'a> {
  lines: std::slice::Iter<'a, &'a [u8]>,
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
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    loop {
      if self.pos < self.buf_len {
        let len = std::cmp::min(buf.len(), self.buf_len - self.pos);
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
        io::Error::new(io::ErrorKind::InvalidData, InvalidBinaryLineError)
      })?;

      let data = &line[1..];
      for chunk in data.chunks(5) {
        if chunk.len() < 5 {
          break;
        }

        let mut val: u32 = 0;
        for &c in chunk {
          val = val.checked_mul(85).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, InvalidBinaryLineError)
          })?;
          val = val
            .checked_add(decode_char(c).ok_or_else(|| {
              io::Error::new(io::ErrorKind::InvalidData, InvalidBinaryLineError)
            })? as u32)
            .ok_or_else(|| {
              io::Error::new(io::ErrorKind::InvalidData, InvalidBinaryLineError)
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

pub fn new_base85_decoder<'a>(
  lines: &'a [&'a [u8]],
) -> ZlibDecoder<Base85Reader<'a>> {
  ZlibDecoder::new(Base85Reader::new(lines))
}

pub fn decode_base85(
  lines: &[&[u8]],
  writer: &mut impl Write,
) -> Result<(), Error> {
  let mut decoder = new_base85_decoder(lines);
  io::copy(&mut decoder, writer).map_err(|e| {
    if e
      .get_ref()
      .map(|r| r.is::<InvalidBinaryLineError>())
      .unwrap_or(false)
    {
      Error::new(ErrorKind::InvalidBinaryFilesLine)
    } else {
      Error::new(ErrorKind::Io(e))
    }
  })?;
  Ok(())
}

fn read_variable_length_int(reader: &mut impl Read) -> Result<u64, Error> {
  let mut result: u64 = 0;
  let mut shift = 0;
  let mut byte_buf = [0u8; 1];

  loop {
    reader.read_exact(&mut byte_buf).map_err(|e| {
      if e.kind() == io::ErrorKind::UnexpectedEof {
        Error::new(ErrorKind::InvalidBinaryPatch)
      } else {
        Error::new(ErrorKind::Io(e))
      }
    })?;

    let byte = byte_buf[0];
    let byte_val = (byte & 0x7f) as u64;
    if shift >= 64 || (byte_val << shift) >> shift != byte_val {
      return Err(Error::new(ErrorKind::InvalidBinaryPatch));
    }
    result |= byte_val << shift;
    shift += 7;
    if (byte & 0x80) == 0 {
      return Ok(result);
    }
  }
}

pub fn apply_delta(
  mut delta: impl Read,
  source: &[u8],
  writer: &mut impl Write,
) -> Result<(), Error> {
  let source_size = read_variable_length_int(&mut delta)?;

  if source_size != source.len() as u64 {
    return Err(Error::new(ErrorKind::BinaryPatchSourceMismatch));
  }

  let target_size = read_variable_length_int(&mut delta)?;

  let mut written: u64 = 0;
  let mut cmd_buf = [0u8; 1];
  let mut literal_buf = [0u8; 127]; // Max literal size is 127 bytes

  loop {
    match delta.read_exact(&mut cmd_buf) {
      Ok(_) => {}
      Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
      Err(e) => return Err(Error::new(ErrorKind::Io(e))),
    }
    let cmd = cmd_buf[0];

    if (cmd & 0x80) != 0 {
      let mut offset: usize = 0;
      let mut size: usize = 0;

      if (cmd & 0x01) != 0 {
        delta
          .read_exact(&mut cmd_buf)
          .map_err(|_| Error::new(ErrorKind::InvalidBinaryPatch))?;
        offset |= cmd_buf[0] as usize;
      }
      if (cmd & 0x02) != 0 {
        delta
          .read_exact(&mut cmd_buf)
          .map_err(|_| Error::new(ErrorKind::InvalidBinaryPatch))?;
        offset |= (cmd_buf[0] as usize) << 8;
      }
      if (cmd & 0x04) != 0 {
        delta
          .read_exact(&mut cmd_buf)
          .map_err(|_| Error::new(ErrorKind::InvalidBinaryPatch))?;
        offset |= (cmd_buf[0] as usize) << 16;
      }
      if (cmd & 0x08) != 0 {
        delta
          .read_exact(&mut cmd_buf)
          .map_err(|_| Error::new(ErrorKind::InvalidBinaryPatch))?;
        offset |= (cmd_buf[0] as usize) << 24;
      }

      if (cmd & 0x10) != 0 {
        delta
          .read_exact(&mut cmd_buf)
          .map_err(|_| Error::new(ErrorKind::InvalidBinaryPatch))?;
        size |= cmd_buf[0] as usize;
      }
      if (cmd & 0x20) != 0 {
        delta
          .read_exact(&mut cmd_buf)
          .map_err(|_| Error::new(ErrorKind::InvalidBinaryPatch))?;
        size |= (cmd_buf[0] as usize) << 8;
      }
      if (cmd & 0x40) != 0 {
        delta
          .read_exact(&mut cmd_buf)
          .map_err(|_| Error::new(ErrorKind::InvalidBinaryPatch))?;
        size |= (cmd_buf[0] as usize) << 16;
      }

      if size == 0 {
        size = 0x10000;
      }

      if offset
        .checked_add(size)
        .is_none_or(|end| end > source.len())
      {
        return Err(Error::new(ErrorKind::InvalidBinaryPatch));
      }

      writer.write_all(&source[offset..offset + size])?;
      written += size as u64;
    } else if cmd != 0 {
      let size = cmd as usize;
      delta
        .read_exact(&mut literal_buf[..size])
        .map_err(|_| Error::new(ErrorKind::InvalidBinaryPatch))?;
      writer.write_all(&literal_buf[..size])?;
      written += size as u64;
    } else {
      return Err(Error::new(ErrorKind::InvalidBinaryPatch));
    }
  }

  if written != target_size {
    return Err(Error::new(ErrorKind::InvalidBinaryPatch));
  }

  Ok(())
}
