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
  buffer: Vec<u8>,
  pos: usize,
}

impl<'a> Base85Reader<'a> {
  pub fn new(lines: &'a [&'a [u8]]) -> Self {
    Self {
      lines: lines.iter(),
      buffer: Vec::new(),
      pos: 0,
    }
  }
}

impl<'a> Read for Base85Reader<'a> {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    loop {
      if self.pos < self.buffer.len() {
        let len = std::cmp::min(buf.len(), self.buffer.len() - self.pos);
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

      self.buffer.clear();
      self.pos = 0;

      let len_char = line[0];
      let len = decode_len_char(len_char).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, InvalidBinaryLineError)
      })?;

      let data = &line[1..];
      for chunk in data.chunks(5) {
        if chunk.len() < 5 {
          break;
        }

        let mut val: u32 = 0;
        for &c in chunk {
          val = val.checked_mul(85).unwrap_or(0);
          val += decode_char(c).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, InvalidBinaryLineError)
          })? as u32;
        }

        self.buffer.push((val >> 24) as u8);
        self.buffer.push((val >> 16) as u8);
        self.buffer.push((val >> 8) as u8);
        self.buffer.push(val as u8);
      }

      if self.buffer.len() > len {
        self.buffer.truncate(len);
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
      Error {
        line: None,
        kind: ErrorKind::InvalidBinaryFilesLine,
      }
    } else {
      Error {
        line: None,
        kind: ErrorKind::Io(e),
      }
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
        Error {
          line: None,
          kind: ErrorKind::InvalidBinaryPatch,
        }
      } else {
        Error {
          line: None,
          kind: ErrorKind::Io(e),
        }
      }
    })?;

    let byte = byte_buf[0];
    let byte_val = (byte & 0x7f) as u64;
    if shift >= 64 || (byte_val << shift) >> shift != byte_val {
      return Err(Error {
        line: None,
        kind: ErrorKind::InvalidBinaryPatch,
      });
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
    return Err(Error {
      line: None,
      kind: ErrorKind::BinaryPatchSourceMismatch,
    });
  }

  let target_size = read_variable_length_int(&mut delta)?;

  let mut written: u64 = 0;
  let mut cmd_buf = [0u8; 1];

  loop {
    match delta.read_exact(&mut cmd_buf) {
      Ok(_) => {}
      Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
      Err(e) => {
        return Err(Error {
          line: None,
          kind: ErrorKind::Io(e),
        })
      }
    }
    let cmd = cmd_buf[0];

    if (cmd & 0x80) != 0 {
      let mut offset: usize = 0;
      let mut size: usize = 0;

      if (cmd & 0x01) != 0 {
        delta.read_exact(&mut cmd_buf).map_err(|_| Error {
          line: None,
          kind: ErrorKind::InvalidBinaryPatch,
        })?;
        offset |= cmd_buf[0] as usize;
      }
      if (cmd & 0x02) != 0 {
        delta.read_exact(&mut cmd_buf).map_err(|_| Error {
          line: None,
          kind: ErrorKind::InvalidBinaryPatch,
        })?;
        offset |= (cmd_buf[0] as usize) << 8;
      }
      if (cmd & 0x04) != 0 {
        delta.read_exact(&mut cmd_buf).map_err(|_| Error {
          line: None,
          kind: ErrorKind::InvalidBinaryPatch,
        })?;
        offset |= (cmd_buf[0] as usize) << 16;
      }
      if (cmd & 0x08) != 0 {
        delta.read_exact(&mut cmd_buf).map_err(|_| Error {
          line: None,
          kind: ErrorKind::InvalidBinaryPatch,
        })?;
        offset |= (cmd_buf[0] as usize) << 24;
      }

      if (cmd & 0x10) != 0 {
        delta.read_exact(&mut cmd_buf).map_err(|_| Error {
          line: None,
          kind: ErrorKind::InvalidBinaryPatch,
        })?;
        size |= cmd_buf[0] as usize;
      }
      if (cmd & 0x20) != 0 {
        delta.read_exact(&mut cmd_buf).map_err(|_| Error {
          line: None,
          kind: ErrorKind::InvalidBinaryPatch,
        })?;
        size |= (cmd_buf[0] as usize) << 8;
      }
      if (cmd & 0x40) != 0 {
        delta.read_exact(&mut cmd_buf).map_err(|_| Error {
          line: None,
          kind: ErrorKind::InvalidBinaryPatch,
        })?;
        size |= (cmd_buf[0] as usize) << 16;
      }

      if size == 0 {
        size = 0x10000;
      }

      if offset
        .checked_add(size)
        .is_none_or(|end| end > source.len())
      {
        return Err(Error {
          line: None,
          kind: ErrorKind::InvalidBinaryPatch,
        });
      }

      writer.write_all(&source[offset..offset + size])?;
      written += size as u64;
    } else if cmd != 0 {
      let size = cmd as usize;
      let mut buf = vec![0u8; size]; // Small allocation for the literal data chunk
      delta.read_exact(&mut buf).map_err(|_| Error {
        line: None,
        kind: ErrorKind::InvalidBinaryPatch,
      })?;
      writer.write_all(&buf)?;
      written += size as u64;
    } else {
      return Err(Error {
        line: None,
        kind: ErrorKind::InvalidBinaryPatch,
      });
    }
  }

  if written != target_size {
    return Err(Error {
      line: None,
      kind: ErrorKind::InvalidBinaryPatch,
    });
  }

  Ok(())
}
