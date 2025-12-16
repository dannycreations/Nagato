use std::io::Read;

use flate2::read::ZlibDecoder;
use nagato_core::error::{Error, ErrorKind};

// Git's base85 alphabet
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

fn decode_base85_raw(lines: &[&[u8]]) -> Result<Vec<u8>, Error> {
  let mut output = Vec::new();

  for line in lines {
    if line.is_empty() {
      continue;
    }

    let len_char = line[0];
    let len = match decode_len_char(len_char) {
      Some(l) => l,
      None => {
        return Err(Error {
          line: None,
          kind: ErrorKind::InvalidBinaryFilesLine,
        })
      }
    };

    let data = &line[1..];
    let mut chunk_out = Vec::new();

    for chunk in data.chunks(5) {
      if chunk.len() < 5 {
        break;
      }

      let mut val: u32 = 0;
      for &c in chunk {
        val = val.checked_mul(85).unwrap_or(0);
        val += match decode_char(c) {
          Some(v) => v as u32,
          None => {
            return Err(Error {
              line: None,
              kind: ErrorKind::InvalidBinaryFilesLine,
            })
          }
        };
      }

      chunk_out.push((val >> 24) as u8);
      chunk_out.push((val >> 16) as u8);
      chunk_out.push((val >> 8) as u8);
      chunk_out.push(val as u8);
    }

    if chunk_out.len() > len {
      chunk_out.truncate(len);
    }
    output.extend_from_slice(&chunk_out);
  }

  Ok(output)
}

pub fn decode_base85(lines: &[&[u8]]) -> Result<Vec<u8>, Error> {
  let compressed = decode_base85_raw(lines)?;
  let mut decoder = ZlibDecoder::new(&compressed[..]);
  let mut decompressed = Vec::new();
  decoder.read_to_end(&mut decompressed).map_err(|e| Error {
    line: None,
    kind: ErrorKind::Io(e),
  })?;
  Ok(decompressed)
}

fn read_variable_length_int(data: &[u8]) -> Result<(u64, usize), Error> {
  let mut result: u64 = 0;
  let mut shift = 0;
  let mut bytes_read = 0;

  for &byte in data {
    bytes_read += 1;
    result |= ((byte & 0x7f) as u64) << shift;
    shift += 7;
    if (byte & 0x80) == 0 {
      return Ok((result, bytes_read));
    }
  }

  Err(Error {
    line: None,
    kind: ErrorKind::InvalidBinaryPatch,
  })
}

pub fn apply_delta(delta: &[u8], source: &[u8]) -> Result<Vec<u8>, Error> {
  let mut pos = 0;

  let (source_size, bytes_read) = read_variable_length_int(&delta[pos..])?;
  pos += bytes_read;

  if source_size != source.len() as u64 {
    return Err(Error {
      line: None,
      kind: ErrorKind::BinaryPatchSourceMismatch,
    });
  }

  let (target_size, bytes_read) = read_variable_length_int(&delta[pos..])?;
  pos += bytes_read;

  let mut output = Vec::with_capacity(target_size as usize);

  while pos < delta.len() {
    let cmd = delta[pos];
    pos += 1;

    if (cmd & 0x80) != 0 {
      let mut offset: usize = 0;
      let mut size: usize = 0;

      if (cmd & 0x01) != 0 {
        if pos >= delta.len() {
          return Err(Error {
            line: None,
            kind: ErrorKind::InvalidBinaryPatch,
          });
        }
        offset |= delta[pos] as usize;
        pos += 1;
      }
      if (cmd & 0x02) != 0 {
        if pos >= delta.len() {
          return Err(Error {
            line: None,
            kind: ErrorKind::InvalidBinaryPatch,
          });
        }
        offset |= (delta[pos] as usize) << 8;
        pos += 1;
      }
      if (cmd & 0x04) != 0 {
        if pos >= delta.len() {
          return Err(Error {
            line: None,
            kind: ErrorKind::InvalidBinaryPatch,
          });
        }
        offset |= (delta[pos] as usize) << 16;
        pos += 1;
      }
      if (cmd & 0x08) != 0 {
        if pos >= delta.len() {
          return Err(Error {
            line: None,
            kind: ErrorKind::InvalidBinaryPatch,
          });
        }
        offset |= (delta[pos] as usize) << 24;
        pos += 1;
      }

      if (cmd & 0x10) != 0 {
        if pos >= delta.len() {
          return Err(Error {
            line: None,
            kind: ErrorKind::InvalidBinaryPatch,
          });
        }
        size |= delta[pos] as usize;
        pos += 1;
      }
      if (cmd & 0x20) != 0 {
        if pos >= delta.len() {
          return Err(Error {
            line: None,
            kind: ErrorKind::InvalidBinaryPatch,
          });
        }
        size |= (delta[pos] as usize) << 8;
        pos += 1;
      }
      if (cmd & 0x40) != 0 {
        if pos >= delta.len() {
          return Err(Error {
            line: None,
            kind: ErrorKind::InvalidBinaryPatch,
          });
        }
        size |= (delta[pos] as usize) << 16;
        pos += 1;
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

      output.extend_from_slice(&source[offset..offset + size]);
    } else if cmd != 0 {
      let size = cmd as usize;
      if pos + size > delta.len() {
        return Err(Error {
          line: None,
          kind: ErrorKind::InvalidBinaryPatch,
        });
      }
      output.extend_from_slice(&delta[pos..pos + size]);
      pos += size;
    } else {
      return Err(Error {
        line: None,
        kind: ErrorKind::InvalidBinaryPatch,
      });
    }
  }

  if output.len() as u64 != target_size {
    return Err(Error {
      line: None,
      kind: ErrorKind::InvalidBinaryPatch,
    });
  }

  Ok(output)
}
