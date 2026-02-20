//! End-to-end sync command integration tests.
//!
//! These cases mirror the remaining Phase 1 integration checklist:
//! basic sync, overwrite/update behavior, dry-run safety, and excludes.

use kopy::commands::sync::run;
use kopy::{Config, DeleteMode};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn config_for(source: &Path, destination: &Path) -> Config {
    Config {
        source: source.to_path_buf(),
        destination: destination.to_path_buf(),
        ..Config::default()
    }
}

#[test]
fn test_basic_sync_empty_destination() {
    let src = TempDir::new().expect("create src tempdir");
    let dst = TempDir::new().expect("create dst tempdir");

    fs::create_dir_all(src.path().join("nested")).expect("create nested source dir");
    fs::write(src.path().join("root.txt"), b"root-content").expect("write root source file");
    fs::write(src.path().join("nested/inner.txt"), b"inner-content")
        .expect("write nested source file");

    run(config_for(src.path(), dst.path())).expect("sync run should succeed");

    assert_eq!(
        fs::read(dst.path().join("root.txt")).expect("read copied root file"),
        b"root-content"
    );
    assert_eq!(
        fs::read(dst.path().join("nested/inner.txt")).expect("read copied nested file"),
        b"inner-content"
    );
}

#[test]
fn test_sync_updates_existing_files() {
    let src = TempDir::new().expect("create src tempdir");
    let dst = TempDir::new().expect("create dst tempdir");

    fs::write(src.path().join("same.txt"), b"new-data").expect("write source version");
    fs::write(dst.path().join("same.txt"), b"old").expect("write destination version");

    run(config_for(src.path(), dst.path())).expect("sync run should succeed");

    assert_eq!(
        fs::read(dst.path().join("same.txt")).expect("read updated destination file"),
        b"new-data"
    );
}

#[test]
fn test_sync_dry_run_makes_no_changes() {
    let src = TempDir::new().expect("create src tempdir");
    let dst = TempDir::new().expect("create dst tempdir");

    fs::write(src.path().join("new.txt"), b"should-not-copy").expect("write source new file");
    fs::write(dst.path().join("old.txt"), b"should-not-delete")
        .expect("write destination old file");

    let mut config = config_for(src.path(), dst.path());
    config.dry_run = true;
    config.delete_mode = DeleteMode::Trash;

    run(config).expect("dry-run should succeed");

    assert!(
        !dst.path().join("new.txt").exists(),
        "dry-run must not copy new files"
    );
    assert!(
        dst.path().join("old.txt").exists(),
        "dry-run must not delete destination-only files"
    );
    assert!(
        !dst.path().join(".kopy_trash").exists(),
        "dry-run must not create trash snapshots"
    );
}

#[test]
fn test_sync_respects_exclude_patterns() {
    let src = TempDir::new().expect("create src tempdir");
    let dst = TempDir::new().expect("create dst tempdir");

    fs::write(src.path().join("keep.txt"), b"keep").expect("write keep file");
    fs::write(src.path().join("ignore.log"), b"ignore").expect("write excluded log file");

    let mut config = config_for(src.path(), dst.path());
    config.exclude_patterns = vec!["*.log".to_string()];

    run(config).expect("sync run with excludes should succeed");

    assert!(dst.path().join("keep.txt").exists());
    assert!(
        !dst.path().join("ignore.log").exists(),
        "excluded file should not be copied"
    );
}

#[test]
fn test_sync_single_file_source_to_existing_destination_directory() {
    let src = TempDir::new().expect("create src tempdir");
    let dst = TempDir::new().expect("create dst tempdir");
    let source_file = src.path().join("one.txt");
    fs::write(&source_file, b"single-file").expect("write source file");

    run(config_for(&source_file, dst.path())).expect("single file sync should succeed");

    assert_eq!(
        fs::read(dst.path().join("one.txt")).expect("read copied destination file"),
        b"single-file"
    );
}

#[test]
fn test_sync_single_file_source_to_destination_file_path() {
    let src = TempDir::new().expect("create src tempdir");
    let dst = TempDir::new().expect("create dst tempdir");
    let source_file = src.path().join("source.txt");
    let destination_file = dst.path().join("renamed.txt");
    fs::write(&source_file, b"renamed-target").expect("write source file");

    run(config_for(&source_file, &destination_file)).expect("single file sync should succeed");

    assert_eq!(
        fs::read(destination_file).expect("read copied destination file"),
        b"renamed-target"
    );
}

#[cfg(unix)]
#[test]
fn test_sync_single_file_checksum_is_idempotent_with_destination_directory() {
    use std::os::unix::fs::MetadataExt;

    let src = TempDir::new().expect("create src tempdir");
    let dst = TempDir::new().expect("create dst tempdir");
    let source_file = src.path().join("one.txt");
    fs::write(&source_file, b"stable-content").expect("write source file");

    let mut config = config_for(&source_file, dst.path());
    config.checksum_mode = true;

    run(config.clone()).expect("first checksum sync should succeed");
    let dest_path = dst.path().join("one.txt");
    let first_inode = fs::metadata(&dest_path).expect("read first metadata").ino();

    run(config).expect("second checksum sync should succeed");
    let second_inode = fs::metadata(&dest_path)
        .expect("read second metadata")
        .ino();

    assert_eq!(
        first_inode, second_inode,
        "idempotent checksum sync should not rewrite destination file"
    );
}
