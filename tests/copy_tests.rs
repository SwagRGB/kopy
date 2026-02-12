//! Tests for atomic file copy operations

use kopy::executor::copy_file_atomic;
use kopy::Config;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn create_test_file(path: &PathBuf, content: &[u8]) {
    let mut file = fs::File::create(path).expect("Failed to create test file");
    file.write_all(content)
        .expect("Failed to write test content");
    file.flush().expect("Failed to flush");
}

fn set_file_mtime(path: &PathBuf, mtime: SystemTime) {
    let filetime_mtime = filetime::FileTime::from_system_time(mtime);
    filetime::set_file_mtime(path, filetime_mtime).expect("Failed to set mtime");
}

#[test]
fn test_copy_basic_content() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    let src_path = root.join("source.txt");
    let content = b"Hello, kopy! This is a test file.";
    create_test_file(&src_path, content);

    let dest_path = root.join("dest.txt");
    let config = Config::default();

    let bytes_copied =
        copy_file_atomic(&src_path, &dest_path, &config).expect("copy_file_atomic should succeed");

    assert_eq!(bytes_copied, content.len() as u64);

    let dest_content = fs::read(&dest_path).expect("Failed to read dest file");
    assert_eq!(dest_content, content);
}

#[test]
fn test_copy_creates_directories() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    let src_path = root.join("source.txt");
    create_test_file(&src_path, b"test content");

    let dest_path = root.join("a/b/c/dest.txt");
    let config = Config::default();

    copy_file_atomic(&src_path, &dest_path, &config)
        .expect("copy_file_atomic should create parent directories");

    assert!(dest_path.parent().unwrap().exists());
    assert!(dest_path.exists());

    let dest_content = fs::read(&dest_path).expect("Failed to read dest file");
    assert_eq!(dest_content, b"test content");
}

#[test]
fn test_copy_preserves_mtime() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    let src_path = root.join("source.txt");
    create_test_file(&src_path, b"test content");

    let mtime = SystemTime::now() - Duration::from_secs(3600);
    set_file_mtime(&src_path, mtime);

    let dest_path = root.join("dest.txt");
    let config = Config::default();

    copy_file_atomic(&src_path, &dest_path, &config).expect("copy_file_atomic should succeed");

    let src_metadata = fs::metadata(&src_path).expect("Failed to read src metadata");
    let dest_metadata = fs::metadata(&dest_path).expect("Failed to read dest metadata");

    let src_mtime = src_metadata.modified().expect("Failed to get src mtime");
    let dest_mtime = dest_metadata.modified().expect("Failed to get dest mtime");

    let diff = if src_mtime > dest_mtime {
        src_mtime.duration_since(dest_mtime).unwrap()
    } else {
        dest_mtime.duration_since(src_mtime).unwrap()
    };

    assert!(
        diff < Duration::from_secs(2),
        "mtime should be preserved (diff: {:?})",
        diff
    );
}

#[test]
fn test_copy_removes_part_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    let src_path = root.join("source.txt");
    create_test_file(&src_path, b"test content");

    let dest_path = root.join("dest.txt");
    let config = Config::default();

    copy_file_atomic(&src_path, &dest_path, &config).expect("copy_file_atomic should succeed");

    let part_path = dest_path.with_extension("part");
    assert!(
        !part_path.exists(),
        ".part file should be removed after successful copy"
    );

    assert!(dest_path.exists());
}

#[test]
fn test_copy_large_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    let src_path = root.join("large.bin");
    let size = 1024 * 1024;
    let content: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
    create_test_file(&src_path, &content);

    let dest_path = root.join("large_copy.bin");
    let config = Config::default();

    let bytes_copied = copy_file_atomic(&src_path, &dest_path, &config)
        .expect("copy_file_atomic should handle large files");

    assert_eq!(bytes_copied, size as u64);

    let dest_content = fs::read(&dest_path).expect("Failed to read dest file");
    assert_eq!(dest_content, content);
}

#[test]
fn test_copy_preserves_permissions() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    let src_path = root.join("source.txt");
    create_test_file(&src_path, b"test content");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&src_path)
            .expect("Failed to get metadata")
            .permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&src_path, perms).expect("Failed to set permissions");
    }

    let dest_path = root.join("dest.txt");
    let config = Config::default();

    copy_file_atomic(&src_path, &dest_path, &config).expect("copy_file_atomic should succeed");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let src_perms = fs::metadata(&src_path)
            .expect("Failed to get src metadata")
            .permissions();
        let dest_perms = fs::metadata(&dest_path)
            .expect("Failed to get dest metadata")
            .permissions();

        assert_eq!(
            src_perms.mode() & 0o777,
            dest_perms.mode() & 0o777,
            "Permissions should be preserved"
        );
    }
}
