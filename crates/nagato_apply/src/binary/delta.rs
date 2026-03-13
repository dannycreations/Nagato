use std::io::{copy, ErrorKind as IoErrorKind, Read, Write};

use nagato_core::{Error, ErrorKind};

fn read_variable_length_int(reader: &mut impl Read) -> Result<u64, Error> {
  let mut result: u64 = 0;
  let mut shift = 0;
  let mut buf = [0u8; 1];

  loop {
    reader.read_exact(&mut buf)?;
    let byte = buf[0];
    let byte_val = (byte & 0x7f) as u64;

    if shift >= 64 {
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
  delta_reader: impl Read,
  source: &[u8],
  writer: &mut (impl Write + ?Sized),
) -> Result<(), Error> {
  let mut delta = std::io::BufReader::with_capacity(8192, delta_reader);

  let source_size = match read_variable_length_int(&mut delta) {
    Ok(s) => s,
    Err(e) if matches!(e.kind, ErrorKind::Io(ref io) if io.kind() == IoErrorKind::UnexpectedEof) =>
    {
      return Err(Error::new(ErrorKind::BinaryPatchSourceMismatch));
    }
    Err(e) => return Err(e),
  };

  if source_size != source.len() as u64 {
    return Err(Error::new(ErrorKind::BinaryPatchSourceMismatch));
  }

  let target_size = read_variable_length_int(&mut delta)?;

  let mut written: u64 = 0;

  let mut cmd_buf = [0u8; 1];
  while delta.read_exact(&mut cmd_buf).is_ok() {
    let cmd = cmd_buf[0];

    if (cmd & 0x80) != 0 {
      let mut offset = 0usize;
      let mut size = 0usize;

      let mut pack_buf = [0u8; 7];
      let n = (cmd & 0x7F).count_ones() as usize;

      delta.read_exact(&mut pack_buf[..n])?;
      let mut p = 0;
      if (cmd & 0x01) != 0 {
        offset = pack_buf[p] as usize;
        p += 1;
      }
      if (cmd & 0x02) != 0 {
        offset |= (pack_buf[p] as usize) << 8;
        p += 1;
      }
      if (cmd & 0x04) != 0 {
        offset |= (pack_buf[p] as usize) << 16;
        p += 1;
      }
      if (cmd & 0x08) != 0 {
        offset |= (pack_buf[p] as usize) << 24;
        p += 1;
      }

      if (cmd & 0x10) != 0 {
        size = pack_buf[p] as usize;
        p += 1;
      }
      if (cmd & 0x20) != 0 {
        size |= (pack_buf[p] as usize) << 8;
        p += 1;
      }
      if (cmd & 0x40) != 0 {
        size |= (pack_buf[p] as usize) << 16;
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
      let size = cmd as u64;
      copy(&mut delta.by_ref().take(size), writer)?;
      written += size;
    } else {
      return Err(Error::new(ErrorKind::InvalidBinaryPatch));
    }
  }

  if written != target_size {
    return Err(Error::new(ErrorKind::InvalidBinaryPatch));
  }

  Ok(())
}
