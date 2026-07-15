use std::{
  fs,
  io::{Cursor, Write},
};

use flate2::{write::ZlibEncoder, Compression};
use nagato_apply::{
  apply_delta, patch_file, Applier, BinaryFragment, BinaryKind, Parser, Patch,
};
use nagato_core::{create_test_fs, ErrorKind, FileSystem};

test_patch_ok!(
  binary_patch_literal,
  initial_fs: {},
  diff: r#"
    diff --git a/binary.dat b/binary.dat
    new file mode 100644
    index 0000000000000000000000000000000000000000..ffbe3091410c3be582675805a98a0118af8e6a6d
    GIT binary patch
    literal 12
    Tc-qTI&B@7ED9<m-Nnrp09%uwz

    literal 0
    Hc-jL100001
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read(root.join("binary.dat")).unwrap(),
      vec![104, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100, 0]
    );
  }
);

test_patch_ok!(
  binary_patch_delta,
  initial_fs: { "binary.dat" => [104, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100, 0] },
  diff: r#"
    diff --git a/binary.dat b/binary.dat
    index ffbe3091410c3be582675805a98a0118af8e6a6d..b08a5c31023a17287d28781fe8ac4af1e26c0f30 100644
    GIT binary patch
    literal 6
    Nc-kw^FUm<_000Qj0x19h

    literal 12
    Tc-qTI&B@7ED9<m-Nnrp09%uwz
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read(root.join("binary.dat")).unwrap(),
      vec![119, 111, 114, 108, 100, 0]
    );
  }
);

test_delta_err!(
  binary_delta_overflows,
  delta: [0x80; 11],
  source: b"",
  expected: ErrorKind::InvalidBinaryPatch
);
test_delta_err!(
  binary_delta_size_mismatch,
  delta: vec![0x05, 0x05, 0x01, b'a'],
  source: b"123",
  expected: ErrorKind::BinaryPatchSourceMismatch
);
test_delta_err!(
  binary_delta_copy_out_of_bounds,
  delta: vec![0x03, 0x03, 0x81, 0x05],
  source: b"123",
  expected: ErrorKind::InvalidBinaryPatch
);

test_patch_err!(
  test_binary_patch_hash_mismatch,
  initial_fs: { "binary.dat" => [0u8, 0, 0] },
  diff: r#"
    diff --git a/binary.dat b/binary.dat
    index abcdef0..1234567 100644
    GIT binary patch
    literal 1
    Wc-qTI&B@7E0000000000
  "#
);

test_patch_ok!(
  test_binary_patch_continues_on_delta_mismatch,
  initial_fs: { "binary.dat" => [1u8, 2, 3] },
  diff: r#"
    diff --git a/binary.dat b/binary.dat
    GIT binary patch
    delta 10
    fcmZ?W&B@7E0000000000

    literal 3
    Wc-qT001
  "#,
  assertions: |_root| {}
);

test_binary_applier_process_ok!(
  binary_applier_process_binary_fragment_selection,
  source: b"source",
  patch: Patch {
    binary_fragments: vec![
      BinaryFragment {
        kind: BinaryKind::Delta,
        size: 5,
        data: vec![b"A00000"],
      },
      BinaryFragment {
        kind: BinaryKind::Literal,
        size: 5,
        data: vec![b"6|SHe00001"],
      },
    ],
    ..Default::default()
  }
);

test_patch_ok!(
  binary_patch_all_zero_hash,
  initial_fs: { "binary.dat" => [1u8, 2, 3] },
  diff: r#"
    diff --git a/binary.dat b/binary.dat
    index 0000000..ffbe309 100644
    GIT binary patch
    literal 6
    Nc-kw^FUm<_000Qj0x19h
  "#,
  assertions: |root| {
    assert_eq!(
      fs::read(root.join("binary.dat")).unwrap(),
      vec![119, 111, 114, 108, 100, 0]
    );
  }
);

#[test]
fn test_binary_applier_fails_immediately_on_invalid_delta() {
  let raw_delta = vec![0x03, 0x03, 0x00];
  let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
  encoder.write_all(&raw_delta).unwrap();
  let compressed = encoder.finish().unwrap();

  let len = compressed.len();
  let mut padded = compressed.clone();
  while padded.len() % 4 != 0 {
    padded.push(0);
  }

  const ENCODE_MAP: &[u8; 85] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";
  let mut encoded = Vec::new();
  let len_char = if len <= 26 {
    b'A' + (len - 1) as u8
  } else {
    b'a' + (len - 27) as u8
  };
  encoded.push(len_char);

  for chunk in padded.as_chunks::<4>().0 {
    let mut val = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    let mut chars = [0u8; 5];
    for i in (0..5).rev() {
      chars[i] = ENCODE_MAP[(val % 85) as usize];
      val /= 85;
    }
    encoded.extend_from_slice(&chars);
  }

  let patch = Patch {
    binary: true,
    binary_fragments: vec![
      BinaryFragment {
        kind: BinaryKind::Delta,
        size: 3,
        data: vec![&encoded],
      },
      BinaryFragment {
        kind: BinaryKind::Literal,
        size: 3,
        data: vec![b"Wc-qT001"], // literal world
      },
    ],
    ..Default::default()
  };

  let mut output = Vec::new();
  let mut applier = Applier::new(&mut output, b"src");
  let res = applier.process_binary(&patch);
  assert_eq!(res.unwrap_err().kind, ErrorKind::InvalidBinaryPatch);
}
