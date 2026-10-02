use std::{
  borrow::Cow,
  fs::{read, write},
  io::Write,
  path::Path,
};
#[cfg(unix)]
use std::{fs::metadata, os::unix::fs::PermissionsExt};

use nagato_core::{
  create_test_fs, get_unique_path, is_dev_null, AtomicWriter, ErrorKind,
  FileSystem,
};

#[cfg(unix)]
#[test]
fn fs_atomic_writer_root_err() {
  assert!(AtomicWriter::new(Path::new("/")).is_err());
}

#[cfg(windows)]
#[test]
fn fs_atomic_writer_root_err() {
  assert!(AtomicWriter::new(Path::new("C:\\")).is_err());
}

#[test]
fn fs_atomic_writer_success() {
  let dir = create_test_fs! {};
  let file_path = dir.path().join("test.txt");

  let mut writer = AtomicWriter::new(&file_path).unwrap();
  writer.write_all(b"content").unwrap();
  writer.commit().unwrap();

  assert_eq!(read(file_path).unwrap(), b"content");
}

#[test]
fn fs_get_unique_path() {
  let dir = create_test_fs! { "test.trim.patch" => "v1" };
  let res = get_unique_path(dir.path(), "test.trim.patch");
  assert_eq!(
    res.file_name().unwrap().to_str().unwrap(),
    "test-1.trim.patch"
  );
}

test_fs_ops_ok!(
  fs_basic_operations,
  check_mode: false,
  assertions: |fs, dir| {
    // Valid path (but not existing)
    assert!(fs.read(b"file.txt").is_err());

    // Write and read
    let path = b"new_file.txt";
    {
      let mut writer = fs.write(path).unwrap();
      writer.write_all(b"data").unwrap();
      writer.commit().unwrap();
    }
    assert!(fs.exists(path));
    assert_eq!(&fs.read(path).unwrap()[..], b"data");

    // Copy clobbers
    write(dir.path().join("dest.txt"), b"old").unwrap();
    fs.copy(path, b"dest.txt").unwrap();
    assert_eq!(read(dir.path().join("dest.txt")).unwrap(), b"data");

    // Remove
    fs.remove(path).unwrap();
    assert!(!fs.exists(path));
  }
);

test_fs_ops_ok!(
  fs_check_mode_isolation,
  check_mode: true,
  assertions: |fs, dir| {
    let path = b"staged_file.txt";
    {
      let mut writer = fs.write(path).unwrap();
      writer.write_all(b"content").unwrap();
      writer.commit().unwrap();
    }

    assert!(fs.exists(path));
    assert!(!dir.path().join("staged_file.txt").exists());

    // Rename in check mode
    fs.rename(path, b"moved.txt").unwrap();
    assert!(!fs.exists(path));
    assert!(fs.exists(b"moved.txt"));
    assert_eq!(&fs.read(b"moved.txt").unwrap()[..], b"content");

    // Remove staged
    fs.remove(b"moved.txt").unwrap();
    assert!(!fs.exists(b"moved.txt"));
    let res = fs.read(b"moved.txt");
    assert!(res.is_err());
    assert!(res.unwrap_err().is_not_found());
  }
);

#[cfg(unix)]
test_fs_invalid_path!(fs_path_root => b"/");

test_fs_invalid_path!(
  fs_path_parent => b"../foo",
  fs_path_parent_nested => b"./foo/../bar",
  fs_path_trailing_dot => b"file.",
  fs_path_trailing_space => b"file ",
  fs_path_dir_space => b"dir /file",
  fs_path_dir_dot => b"dir./file",
  fs_path_short_name => b"progra~1",
);

test_fs_invalid_path!(
  fs_reserved_con => b"CON",
  fs_reserved_prn => b"PRN",
  fs_reserved_aux => b"AUX",
  fs_reserved_nul => b"NUL",
  fs_reserved_com1 => b"COM1",
  fs_reserved_lpt9 => b"LPT9",
  fs_reserved_clock_dollar => b"CLOCK$",
  fs_reserved_aux_txt => b"aux.txt",
  fs_reserved_aux_file => b"AUX/file",
);

#[cfg(unix)]
test_fs_ops_ok!(
  fs_permissions_sanitization,
  check_mode: false,
  assertions: |fs, dir| {
    let path = b"restricted.sh";
    {
      let mut writer = fs.write(path).unwrap();
      writer.write_all(b"echo hello").unwrap();
      writer.commit().unwrap();
    }

    // Try to set SUID/SGID bits (06755)
    fs.set_permissions(path, 0o6755).unwrap();

    let metadata = metadata(dir.path().join("restricted.sh")).unwrap();
    let mode = metadata.permissions().mode();

    // Verify SUID (04000) and SGID (02000) are stripped
    assert_eq!(mode & 0o6000, 0);
    assert_eq!(mode & 0o777, 0o755);
  }
);

test_fs_ops_ok!(
  fs_path_cache_limit,
  check_mode: false,
  assertions: |fs, _dir| {
    // Fill cache to limit
    for i in 0..10_000 {
      let path = format!("file_{}.txt", i);
      fs.exists(path.as_bytes());
    }

    // Add one more
    let overflow = b"overflow.txt";
    fs.exists(overflow);

    // Verify it doesn't crash and works correctly (just not cached)
    assert!(!fs.exists(overflow));
  }
);

#[test]
fn test_is_dev_null() {
  assert!(is_dev_null(b"dev/null"));
  assert!(is_dev_null(b"/dev/null"));
  assert!(!is_dev_null(b"not/dev/null"));
  assert!(is_dev_null(&Cow::Borrowed(b"dev/null" as &[u8])));
  assert!(!is_dev_null(&Cow::Borrowed(b"other" as &[u8])));
}

test_fs_ops_ok!(
  fs_rename_identical_paths,
  check_mode: false,
  assertions: |fs, dir| {
    let path = b"identical.txt";
    {
      let mut writer = fs.write(path).unwrap();
      writer.write_all(b"data").unwrap();
      writer.commit().unwrap();
    }
    // Rename identical paths should be a no-op and succeed
    assert!(fs.rename(path, path).is_ok());
    assert!(fs.exists(path));
  }
);

test_fs_ops_ok!(
  fs_remove_non_existent_file,
  check_mode: false,
  assertions: |fs, _dir| {
    let path = b"non_existent.txt";
    // Removing a non-existent file should succeed (no-op)
    assert!(fs.remove(path).is_ok());
  }
);

test_fs_ops_ok!(
  fs_check_mode_remove_non_existent_file,
  check_mode: true,
  assertions: |fs, _dir| {
    let path = b"non_existent.txt";
    // Removing a non-existent file in check mode should succeed
    assert!(fs.remove(path).is_ok());
  }
);

test_fs_ops_ok!(
  fs_tilde_restriction_valid,
  check_mode: false,
  assertions: |fs, _dir| {
    let path = b"file~with~tilde.txt";
    // Path with tildes not followed by digits should be valid
    assert!(!fs.exists(path));
  }
);

test_fs_invalid_path!(
  fs_tilde_followed_by_digit => b"file~1",
  fs_tilde_followed_by_digit_nested => b"foo~2bar/file",
  fs_multiple_tilde_with_digit => b"foo~bar~3",
);

test_fs_ops_ok!(
  fs_copy_rename_non_existent_errors,
  check_mode: false,
  assertions: |fs, _dir| {
    assert!(fs.copy(b"non_existent.txt", b"dest.txt").is_err());
    assert!(fs.rename(b"non_existent.txt", b"dest.txt").is_err());
  }
);
