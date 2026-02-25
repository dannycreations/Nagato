use std::fs;

use nagato_apply::{BinaryFragment, BinaryKind, Patch};
use nagato_core::ErrorKind;

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
    ]
    .into_boxed_slice(),
    ..Default::default()
  }
);
