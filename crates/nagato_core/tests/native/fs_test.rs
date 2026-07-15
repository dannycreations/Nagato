#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
  borrow::Cow,
  fs::{read, write},
  io::{Error as StdIoError, ErrorKind as StdIoErrorKind, Write},
  path::Path,
};

use nagato_core::{
  create_test_fs, get_unique_path, AtomicWriter, Error, ErrorKind, FileSystem,
  IgnoreNotFound, IsDevNull,
};

#[cfg(unix)]
test_atomic_writer_err!(
  fs_atomic_writer_root_err,
  path: "/"
);
#[cfg(windows)]
test_atomic_writer_err!(
  fs_atomic_writer_root_err,
  path: "C:\\"
);

test_atomic_writer_ok!(
  fs_atomic_writer_success,
  content: b"content"
);

test_fs_get_unique_path_ok!(
  fs_get_unique_path,
  initial_fs: { "test.trim.patch" => "v1" },
  base_name: "test.trim.patch",
  expected_file_name: "test-1.trim.patch"
);

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
  assert!(b"dev/null".is_dev_null());
  assert!(b"/dev/null".is_dev_null());
  assert!(!b"not/dev/null".is_dev_null());
  assert!(Cow::Borrowed(b"dev/null" as &[u8]).is_dev_null());
  assert!(!Cow::Borrowed(b"other" as &[u8]).is_dev_null());
}

#[test]
fn test_ignore_not_found() {
  let err_not_found = Error::new(ErrorKind::Io(StdIoError::new(
    StdIoErrorKind::NotFound,
    "file not found",
  )));
  let res_not_found: Result<Vec<u8>, Error> = Err(err_not_found);
  assert_eq!(res_not_found.ignore_not_found().unwrap(), Vec::<u8>::new());

  let err_other = Error::new(ErrorKind::AlreadyExists);
  let res_other: Result<Vec<u8>, Error> = Err(err_other);
  assert!(res_other.ignore_not_found().is_err());

  let res_ok: Result<Vec<u8>, Error> = Ok(vec![1, 2, 3]);
  assert_eq!(res_ok.ignore_not_found().unwrap(), vec![1, 2, 3]);
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
