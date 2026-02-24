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
  let mut delta_buf = Vec::with_capacity(4096);
  delta_reader.read_to_end(&mut delta_buf)?;
  let mut delta = delta_buf.as_slice();

  let source_size = read_variable_length_int(&mut delta)?;

  if source_size != source.len() as u64 {
    return Err(Error::new(ErrorKind::BinaryPatchSourceMismatch));
  }

  let target_size = read_variable_length_int(&mut delta)?;

  let mut written: u64 = 0;

  while !delta.is_empty() {
    let cmd = delta[0];
    delta = &delta[1..];

    if (cmd & 0x80) != 0 {
      let mut offset: usize = 0;
      let mut size: usize = 0;

      for i in 0..4 {
        if (cmd & (1 << i)) != 0 {
          let (&byte, rest) = delta
            .split_first()
            .ok_or_else(|| Error::new(ErrorKind::InvalidBinaryPatch))?;
          delta = rest;
          offset |= (byte as usize) << (i * 8);
        }
      }

      for i in 0..3 {
        if (cmd & (1 << (i + 4))) != 0 {
          let (&byte, rest) = delta
            .split_first()
            .ok_or_else(|| Error::new(ErrorKind::InvalidBinaryPatch))?;
          delta = rest;
          size |= (byte as usize) << (i * 8);
        }
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
