use std::fs;

test_patch_ok!(
  applies_binary_patch_literal,
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
  applies_binary_patch_delta,
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
