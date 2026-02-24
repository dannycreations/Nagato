use std::io::{Read, Write};

use nagato_core::{Error, ErrorKind};

fn read_variable_length_int(data: &mut &[u8]) -> Result<u64, Error> {
  let mut result: u64 = 0;
  let mut shift = 0;

  while let Some((&byte, rest)) = data.split_first() {
    *data = rest;
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
  Err(Error::new(ErrorKind::InvalidBinaryPatch))
}

pub fn apply_delta(
  mut delta_reader: impl Read,
  source: &[u8],
  writer: &mut (impl Write + ?Sized),
) -> Result<(), Error> {
  let mut delta_buf = Vec::new();
  delta_reader.read_to_end(&mut delta_buf)?;
  let mut delta = delta_buf.as_slice();

  let source_size = read_variable_length_int(&mut delta)?;

  if source_size != source.len() as u64 {
    return Err(Error::new(ErrorKind::BinaryPatchSourceMismatch));
  }

  let target_size = read_variable_length_int(&mut delta)?;

  let mut written: u64 = 0;

  while let Some((&cmd, rest)) = delta.split_first() {
    delta = rest;

    if (cmd & 0x80) != 0 {
      let mut offset: usize = 0;
      let mut size: usize = 0;

      if (cmd & 0x01) != 0 {
        let (&byte, rest) = delta
          .split_first()
          .ok_or_else(|| Error::new(ErrorKind::InvalidBinaryPatch))?;
        delta = rest;
        offset = byte as usize;
      }
      if (cmd & 0x02) != 0 {
        let (&byte, rest) = delta
          .split_first()
          .ok_or_else(|| Error::new(ErrorKind::InvalidBinaryPatch))?;
        delta = rest;
        offset |= (byte as usize) << 8;
      }
      if (cmd & 0x04) != 0 {
        let (&byte, rest) = delta
          .split_first()
          .ok_or_else(|| Error::new(ErrorKind::InvalidBinaryPatch))?;
        delta = rest;
        offset |= (byte as usize) << 16;
      }
      if (cmd & 0x08) != 0 {
        let (&byte, rest) = delta
          .split_first()
          .ok_or_else(|| Error::new(ErrorKind::InvalidBinaryPatch))?;
        delta = rest;
        offset |= (byte as usize) << 24;
      }

      if (cmd & 0x10) != 0 {
        let (&byte, rest) = delta
          .split_first()
          .ok_or_else(|| Error::new(ErrorKind::InvalidBinaryPatch))?;
        delta = rest;
        size = byte as usize;
      }
      if (cmd & 0x20) != 0 {
        let (&byte, rest) = delta
          .split_first()
          .ok_or_else(|| Error::new(ErrorKind::InvalidBinaryPatch))?;
        delta = rest;
        size |= (byte as usize) << 8;
      }
      if (cmd & 0x40) != 0 {
        let (&byte, rest) = delta
          .split_first()
          .ok_or_else(|| Error::new(ErrorKind::InvalidBinaryPatch))?;
        delta = rest;
        size |= (byte as usize) << 16;
      }

      if size == 0 {
        size = 0x10000;
      }

      let end = offset
        .checked_add(size)
        .ok_or_else(|| Error::new(ErrorKind::InvalidBinaryPatch))?;
      if end > source.len() {
        return Err(Error::new(ErrorKind::InvalidBinaryPatch));
      }

      writer.write_all(&source[offset..end])?;
      written += size as u64;
    } else if cmd != 0 {
      let size = cmd as usize;
      if delta.len() < size {
        return Err(Error::new(ErrorKind::InvalidBinaryPatch));
      }
      writer.write_all(&delta[..size])?;
      delta = &delta[size..];
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
