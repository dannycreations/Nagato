use std::io::{ErrorKind as IoErrorKind, Read, Write};

use nagato_core::{Error, ErrorKind};

fn read_variable_length_int(reader: &mut impl Read) -> Result<u64, Error> {
  let mut result: u64 = 0;
  let mut shift = 0;
  let mut byte_buf = [0u8; 1];

  loop {
    reader.read_exact(&mut byte_buf).map_err(|e| {
      if e.kind() == IoErrorKind::UnexpectedEof {
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
      Err(e) if e.kind() == IoErrorKind::UnexpectedEof => break,
      Err(e) => return Err(Error::new(ErrorKind::Io(e))),
    }
    let cmd = cmd_buf[0];

    if (cmd & 0x80) != 0 {
      let mut offset: usize = 0;
      let mut size: usize = 0;

      for i in 0..4 {
        if (cmd & (1 << i)) != 0 {
          delta
            .read_exact(&mut cmd_buf)
            .map_err(|_| Error::new(ErrorKind::InvalidBinaryPatch))?;
          offset |= (cmd_buf[0] as usize) << (i * 8);
        }
      }

      for i in 0..3 {
        if (cmd & (1 << (i + 4))) != 0 {
          delta
            .read_exact(&mut cmd_buf)
            .map_err(|_| Error::new(ErrorKind::InvalidBinaryPatch))?;
          size |= (cmd_buf[0] as usize) << (i * 8);
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
