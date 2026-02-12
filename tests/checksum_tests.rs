//! Content hashing tests (checksum mode)
//!
//! Tests for Blake3 content hashing and checksum-based comparison

use kopy::diff::generate_sync_plan;
use kopy::hash::compute_hash;
use kopy::types::{DeleteMode, FileEntry, FileTree};
use kopy::Config;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};
use tempfile::TempDir;

fn create_test_config(checksum_mode: bool) -> Config {
    Config {
        source: PathBuf::from("/src"),
        destination: PathBuf::from("/dest"),
        delete_mode: DeleteMode::None,
        dry_run: false,
        checksum_mode,
        exclude_patterns: vec![],
        include_patterns: vec![],
        threads: 4,
        bandwidth_limit: None,
        backup_dir: None,
        watch: false,
        watch_settle: 2,
    }
}

fn create_temp_file(dir: &TempDir, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, content).expect("Failed to write test file");
    path
}

#[test]
fn test_compute_hash_basic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = create_temp_file(&temp_dir, "test.txt", b"Hello, World!");

    let hash = compute_hash(&file_path).expect("Failed to compute hash");

    assert_eq!(hash.len(), 32);
}

#[test]
fn test_compute_hash_deterministic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file1 = create_temp_file(&temp_dir, "file1.txt", b"Same content");
    let file2 = create_temp_file(&temp_dir, "file2.txt", b"Same content");

    let hash1 = compute_hash(&file1).expect("Failed to compute hash1");
    let hash2 = compute_hash(&file2).expect("Failed to compute hash2");

    assert_eq!(hash1, hash2);
}

#[test]
fn test_compute_hash_different_content() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file1 = create_temp_file(&temp_dir, "file1.txt", b"Content A");
    let file2 = create_temp_file(&temp_dir, "file2.txt", b"Content B");

    let hash1 = compute_hash(&file1).expect("Failed to compute hash1");
    let hash2 = compute_hash(&file2).expect("Failed to compute hash2");

    assert_ne!(hash1, hash2);
}

#[test]
fn test_compute_hash_empty_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = create_temp_file(&temp_dir, "empty.txt", b"");

    let hash = compute_hash(&file_path).expect("Failed to compute hash of empty file");

    assert_eq!(hash.len(), 32);
}

#[test]
fn test_compute_hash_large_file() {
    let temp_dir = tempfile::tempdir().unwrap();

    let content = vec![0x42u8; 1024 * 1024];
    let file_path = create_temp_file(&temp_dir, "large.bin", &content);

    let hash = compute_hash(&file_path).expect("Failed to compute hash of large file");

    assert_eq!(hash.len(), 32);
}

#[test]
fn test_compute_hash_nonexistent_file() {
    let path = PathBuf::from("/nonexistent/file.txt");

    let result = compute_hash(&path);

    assert!(result.is_err());
}

#[test]
fn test_checksum_mismatch() {
    let src_dir = tempfile::tempdir().unwrap();
    let dest_dir = tempfile::tempdir().unwrap();

    let src_file = create_temp_file(&src_dir, "file.txt", b"Content AAAA");
    let dest_file = create_temp_file(&dest_dir, "file.txt", b"Content BBBB");

    let mut src_tree = FileTree::new(src_dir.path().to_path_buf());
    let src_metadata = fs::metadata(&src_file).unwrap();
    src_tree.insert(
        PathBuf::from("file.txt"),
        FileEntry::new(
            PathBuf::from("file.txt"),
            src_metadata.len(),
            src_metadata.modified().unwrap(),
            0o644,
        ),
    );

    let mut dest_tree = FileTree::new(dest_dir.path().to_path_buf());
    let dest_metadata = fs::metadata(&dest_file).unwrap();
    dest_tree.insert(
        PathBuf::from("file.txt"),
        FileEntry::new(
            PathBuf::from("file.txt"),
            dest_metadata.len(),
            dest_metadata.modified().unwrap(),
            0o644,
        ),
    );

    let mut config = create_test_config(true);
    config.source = src_dir.path().to_path_buf();
    config.destination = dest_dir.path().to_path_buf();

    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(
        plan.stats.overwrite_count, 1,
        "Checksum mismatch should trigger Overwrite"
    );
    assert!(plan.actions[0].is_overwrite());
}

#[test]
fn test_checksum_match() {
    let src_dir = tempfile::tempdir().unwrap();
    let dest_dir = tempfile::tempdir().unwrap();

    let content = b"Identical content here";
    let src_file = create_temp_file(&src_dir, "file.txt", content);
    let dest_file = create_temp_file(&dest_dir, "file.txt", content);

    let mut src_tree = FileTree::new(src_dir.path().to_path_buf());
    let src_metadata = fs::metadata(&src_file).unwrap();
    src_tree.insert(
        PathBuf::from("file.txt"),
        FileEntry::new(
            PathBuf::from("file.txt"),
            src_metadata.len(),
            UNIX_EPOCH + Duration::from_secs(2000),
            0o644,
        ),
    );

    let mut dest_tree = FileTree::new(dest_dir.path().to_path_buf());
    let dest_metadata = fs::metadata(&dest_file).unwrap();
    dest_tree.insert(
        PathBuf::from("file.txt"),
        FileEntry::new(
            PathBuf::from("file.txt"),
            dest_metadata.len(),
            UNIX_EPOCH + Duration::from_secs(1000),
            0o644,
        ),
    );

    let mut config = create_test_config(true);
    config.source = src_dir.path().to_path_buf();
    config.destination = dest_dir.path().to_path_buf();

    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(
        plan.stats.skip_count, 1,
        "Checksum match should trigger Skip even with different mtimes"
    );
    assert_eq!(plan.stats.overwrite_count, 0);
}

#[test]
fn test_checksum_mode_off_uses_mtime() {
    let src_dir = tempfile::tempdir().unwrap();
    let dest_dir = tempfile::tempdir().unwrap();

    let _src_file = create_temp_file(&src_dir, "file.txt", b"Content AAAA");
    let _dest_file = create_temp_file(&dest_dir, "file.txt", b"Content BBBB");

    let mut src_tree = FileTree::new(src_dir.path().to_path_buf());
    src_tree.insert(
        PathBuf::from("file.txt"),
        FileEntry::new(
            PathBuf::from("file.txt"),
            12,
            UNIX_EPOCH + Duration::from_secs(2000), // Source newer
            0o644,
        ),
    );

    let mut dest_tree = FileTree::new(dest_dir.path().to_path_buf());
    dest_tree.insert(
        PathBuf::from("file.txt"),
        FileEntry::new(
            PathBuf::from("file.txt"),
            12,
            UNIX_EPOCH + Duration::from_secs(1000), // Dest older
            0o644,
        ),
    );

    let config = create_test_config(false);
    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(plan.stats.overwrite_count, 1);
}

#[test]
fn test_size_mismatch_always_overwrites() {
    let src_dir = tempfile::tempdir().unwrap();
    let dest_dir = tempfile::tempdir().unwrap();

    let src_file = create_temp_file(&src_dir, "file.txt", b"Short");
    let dest_file = create_temp_file(&dest_dir, "file.txt", b"Much longer content");

    let mut src_tree = FileTree::new(src_dir.path().to_path_buf());
    let src_metadata = fs::metadata(&src_file).unwrap();
    src_tree.insert(
        PathBuf::from("file.txt"),
        FileEntry::new(
            PathBuf::from("file.txt"),
            src_metadata.len(),
            src_metadata.modified().unwrap(),
            0o644,
        ),
    );

    let mut dest_tree = FileTree::new(dest_dir.path().to_path_buf());
    let dest_metadata = fs::metadata(&dest_file).unwrap();
    dest_tree.insert(
        PathBuf::from("file.txt"),
        FileEntry::new(
            PathBuf::from("file.txt"),
            dest_metadata.len(),
            dest_metadata.modified().unwrap(),
            0o644,
        ),
    );

    let mut config = create_test_config(true);
    config.source = src_dir.path().to_path_buf();
    config.destination = dest_dir.path().to_path_buf();

    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(
        plan.stats.overwrite_count, 1,
        "Size mismatch should trigger overwrite even in checksum mode"
    );
}
