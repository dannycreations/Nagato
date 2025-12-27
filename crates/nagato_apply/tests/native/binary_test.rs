use std::fs;

use nagato_apply::Parser;
use nagato_core::{ErrorKind, FileSystem};
use tempfile::Builder;

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

#[test]
fn fails_on_base85_overflow() {
  let diff = indoc::indoc!(
    r#"
      diff --git a/binary.dat b/binary.dat
      new file mode 100644
      index 0000000000000000000000000000000000000000..ffbe3091410c3be582675805a98a0118af8e6a6d
      GIT binary patch
      literal 4
      ~~~~~
      
      literal 0
      Hc-jL100001
      "#
  );

  let dir = Builder::new().prefix("test_overflow").tempdir().unwrap();
  let fs = FileSystem::new(dir.path());

  let patch = Parser::new(diff.as_bytes()).next().unwrap().unwrap();

  let result = nagato_apply::patch_file(&fs, patch, false, false);

  match result {
    Err(e) => {
      let is_invalid_binary =
        matches!(e.kind, ErrorKind::InvalidBinaryFilesLine);
      let is_io_invalid_data = if let ErrorKind::Io(io_err) = &e.kind {
        io_err.kind() == std::io::ErrorKind::InvalidData
      } else {
        false
      };

      assert!(
        is_invalid_binary || is_io_invalid_data,
        "Expected InvalidBinaryFilesLine or IO InvalidData, got {:?}",
        e
      );
    }
    Ok(_) => panic!("Expected overflow error"),
  }
}
