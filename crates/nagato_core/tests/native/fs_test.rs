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
    std::fs::write(dir.path().join("dest.txt"), b"old").unwrap();
    fs.copy(path, b"dest.txt").unwrap();
    assert_eq!(std::fs::read(dir.path().join("dest.txt")).unwrap(), b"data");

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
    use std::os::unix::fs::PermissionsExt;
    let path = b"restricted.sh";
    {
      let mut writer = fs.write(path).unwrap();
      writer.write_all(b"echo hello").unwrap();
      writer.commit().unwrap();
    }

    // Try to set SUID/SGID bits (06755)
    fs.set_permissions(path, 0o6755).unwrap();

    let metadata = std::fs::metadata(dir.path().join("restricted.sh")).unwrap();
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
