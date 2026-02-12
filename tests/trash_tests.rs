//! Tests for trash-based deletion functionality

use kopy::executor::trash::move_to_trash;
use kopy::Config;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn create_test_file(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
    let file_path = dir.join(name);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).expect("Failed to create parent dirs");
    }
    fs::write(&file_path, content).expect("Failed to create test file");
    file_path
}

#[test]
fn test_trash_move_basic() {
    let dest_dir = TempDir::new().expect("Failed to create temp dir");
    let dest_path = dest_dir.path();

    let test_file = create_test_file(dest_path, "subdir/test.txt", "Hello, kopy!");
    let relative_path = Path::new("subdir/test.txt");

    let config = Config::default();

    let result = move_to_trash(&test_file, dest_path, relative_path, &config);

    assert!(result.is_ok(), "move_to_trash failed: {:?}", result.err());

    assert!(!test_file.exists(), "Original file should be deleted");

    let trash_root = dest_path.join(".kopy_trash");
    assert!(trash_root.exists(), "Trash root directory should exist");

    let trash_entries: Vec<_> = fs::read_dir(&trash_root)
        .expect("Failed to read trash dir")
        .filter_map(|e| e.ok())
        .collect();

    assert_eq!(
        trash_entries.len(),
        1,
        "Should have exactly one trash snapshot"
    );

    let snapshot_dir = trash_entries[0].path();
    let trashed_file = snapshot_dir.join("subdir/test.txt");

    assert!(trashed_file.exists(), "Trashed file should exist");
    let content = fs::read_to_string(&trashed_file).expect("Failed to read trashed file");
    assert_eq!(content, "Hello, kopy!", "Trashed file content should match");
}

#[test]
fn test_trash_manifest() {
    use serde_json;

    let dest_dir = TempDir::new().expect("Failed to create temp dir");
    let dest_path = dest_dir.path();

    let test_file = create_test_file(dest_path, "document.pdf", "PDF content here");
    let relative_path = Path::new("document.pdf");

    let config = Config::default();

    let result = move_to_trash(&test_file, dest_path, relative_path, &config);
    assert!(result.is_ok());

    let trash_root = dest_path.join(".kopy_trash");
    let trash_entries: Vec<_> = fs::read_dir(&trash_root)
        .expect("Failed to read trash dir")
        .filter_map(|e| e.ok())
        .collect();

    let snapshot_dir = trash_entries[0].path();
    let manifest_path = snapshot_dir.join("MANIFEST.json");

    assert!(manifest_path.exists(), "MANIFEST.json should exist");

    let manifest_content = fs::read_to_string(&manifest_path).expect("Failed to read manifest");

    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_content).expect("MANIFEST.json should be valid JSON");

    let files = manifest["files"]
        .as_array()
        .expect("MANIFEST should have 'files' array");

    assert_eq!(files.len(), 1, "Should have one deleted file entry");

    let deleted_file = &files[0];

    assert_eq!(
        deleted_file["original_path"].as_str(),
        Some("document.pdf"),
        "original_path should match"
    );

    assert!(
        deleted_file["trash_path"].as_str().is_some(),
        "trash_path should be present"
    );

    assert!(
        deleted_file["deleted_at"].as_str().is_some(),
        "deleted_at timestamp should be present"
    );

    let timestamp = deleted_file["deleted_at"].as_str().unwrap();
    assert!(
        timestamp.contains('-') && timestamp.contains(':'),
        "Timestamp should be in ISO 8601 format, got: {}",
        timestamp
    );

    assert_eq!(
        deleted_file["size"].as_u64(),
        Some(16),
        "size should match original file size"
    );
}

#[test]
fn test_trash_multiple_files() {
    let dest_dir = TempDir::new().expect("Failed to create temp dir");
    let dest_path = dest_dir.path();

    let file1 = create_test_file(dest_path, "file1.txt", "Content 1");
    let file2 = create_test_file(dest_path, "dir/file2.txt", "Content 2");

    let config = Config::default();

    let result1 = move_to_trash(&file1, dest_path, Path::new("file1.txt"), &config);
    let result2 = move_to_trash(&file2, dest_path, Path::new("dir/file2.txt"), &config);

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    let trash_root = dest_path.join(".kopy_trash");

    let trash_entries: Vec<_> = fs::read_dir(&trash_root)
        .expect("Failed to read trash dir")
        .filter_map(|e| e.ok())
        .collect();

    assert!(
        !trash_entries.is_empty(),
        "Should have at least one trash snapshot"
    );

    let mut total_files = 0;
    for entry in trash_entries {
        let manifest_path = entry.path().join("MANIFEST.json");
        if manifest_path.exists() {
            let manifest_content = fs::read_to_string(&manifest_path).unwrap();
            let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
            let files = manifest["files"].as_array().unwrap();
            total_files += files.len();
        }
    }

    assert_eq!(
        total_files, 2,
        "Should have 2 total files across all manifests"
    );
}
