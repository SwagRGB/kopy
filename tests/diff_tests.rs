//! Diff engine integration tests
//!
//! Tests for the diff engine's ability to compare file trees and generate sync plans.

use kopy::diff::{compare_files, generate_sync_plan};
use kopy::types::{DeleteMode, FileEntry, FileTree, SyncAction};
use kopy::Config;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

// ═══════════════════════════════════════════════════════════
// Test Helpers
// ═══════════════════════════════════════════════════════════

fn create_test_entry(name: &str, size: u64, mtime_secs: u64) -> FileEntry {
    FileEntry::new(
        PathBuf::from(name),
        size,
        UNIX_EPOCH + Duration::from_secs(mtime_secs),
        0o644,
    )
}

fn create_test_config(delete_mode: DeleteMode) -> Config {
    Config {
        source: PathBuf::from("/src"),
        destination: PathBuf::from("/dest"),
        delete_mode,
        dry_run: false,
        checksum_mode: false,
        exclude_patterns: vec![],
        include_patterns: vec![],
        threads: 4,
        bandwidth_limit: None,
        backup_dir: None,
        watch: false,
        watch_settle: 2,
    }
}

// ═══════════════════════════════════════════════════════════
// compare_files() Tests
// ═══════════════════════════════════════════════════════════

#[test]
fn test_compare_size_mismatch() {
    let src = create_test_entry("file.txt", 1024, 1000);
    let dest = create_test_entry("file.txt", 2048, 1000); // Different size
    let config = create_test_config(DeleteMode::None);

    let action = compare_files(&src, &dest, &config);

    assert!(
        action.is_overwrite(),
        "Size mismatch should trigger Overwrite"
    );
    if let SyncAction::Overwrite(entry) = action {
        assert_eq!(entry.size, 1024);
    }
}

#[test]
fn test_compare_src_newer() {
    let src = create_test_entry("file.txt", 1024, 2000); // Newer
    let dest = create_test_entry("file.txt", 1024, 1000);
    let config = create_test_config(DeleteMode::None);

    let action = compare_files(&src, &dest, &config);

    assert!(
        action.is_overwrite(),
        "Source newer should trigger Overwrite"
    );
}

#[test]
fn test_compare_dest_newer() {
    let src = create_test_entry("file.txt", 1024, 1000);
    let dest = create_test_entry("file.txt", 1024, 2000); // Newer
    let config = create_test_config(DeleteMode::None);

    let action = compare_files(&src, &dest, &config);

    assert!(
        action.is_skip(),
        "Dest newer should Skip (Phase 1 conflict avoidance)"
    );
}

#[test]
fn test_compare_identical() {
    let src = create_test_entry("file.txt", 1024, 1000);
    let dest = create_test_entry("file.txt", 1024, 1000); // Identical
    let config = create_test_config(DeleteMode::None);

    let action = compare_files(&src, &dest, &config);

    assert!(action.is_skip(), "Identical files should Skip");
}

// ═══════════════════════════════════════════════════════════
// generate_sync_plan() Tests
// ═══════════════════════════════════════════════════════════

#[test]
fn test_diff_copy_new() {
    // Source has file, dest is empty
    let mut src_tree = FileTree::new(PathBuf::from("/src"));
    src_tree.insert(
        PathBuf::from("new_file.txt"),
        create_test_entry("new_file.txt", 1024, 1000),
    );

    let dest_tree = FileTree::new(PathBuf::from("/dest"));
    let config = create_test_config(DeleteMode::None);

    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(plan.stats.copy_count, 1);
    assert_eq!(plan.stats.total_files, 1);
    assert_eq!(plan.stats.total_bytes, 1024);
    assert_eq!(plan.actions.len(), 1);
    assert!(plan.actions[0].is_copy_new());
}

#[test]
fn test_diff_overwrite_size() {
    // Both have file, but sizes differ
    let mut src_tree = FileTree::new(PathBuf::from("/src"));
    src_tree.insert(
        PathBuf::from("file.txt"),
        create_test_entry("file.txt", 2048, 1000),
    );

    let mut dest_tree = FileTree::new(PathBuf::from("/dest"));
    dest_tree.insert(
        PathBuf::from("file.txt"),
        create_test_entry("file.txt", 1024, 1000), // Different size
    );

    let config = create_test_config(DeleteMode::None);
    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(plan.stats.overwrite_count, 1);
    assert_eq!(plan.stats.total_files, 1);
    assert_eq!(plan.stats.total_bytes, 2048);
    assert!(plan.actions[0].is_overwrite());
}

#[test]
fn test_diff_overwrite_newer() {
    // Both have file, same size, source is newer
    let mut src_tree = FileTree::new(PathBuf::from("/src"));
    src_tree.insert(
        PathBuf::from("file.txt"),
        create_test_entry("file.txt", 1024, 2000), // Newer
    );

    let mut dest_tree = FileTree::new(PathBuf::from("/dest"));
    dest_tree.insert(
        PathBuf::from("file.txt"),
        create_test_entry("file.txt", 1024, 1000),
    );

    let config = create_test_config(DeleteMode::None);
    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(plan.stats.overwrite_count, 1);
    assert!(plan.actions[0].is_overwrite());
}

#[test]
fn test_diff_skip_older() {
    // Both have file, dest is newer (conflict)
    let mut src_tree = FileTree::new(PathBuf::from("/src"));
    src_tree.insert(
        PathBuf::from("file.txt"),
        create_test_entry("file.txt", 1024, 1000),
    );

    let mut dest_tree = FileTree::new(PathBuf::from("/dest"));
    dest_tree.insert(
        PathBuf::from("file.txt"),
        create_test_entry("file.txt", 1024, 2000), // Newer
    );

    let config = create_test_config(DeleteMode::None);
    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(plan.stats.skip_count, 1);
    assert_eq!(plan.stats.overwrite_count, 0);
    assert_eq!(plan.stats.total_files, 0); // Skips don't count as transfers
}

#[test]
fn test_diff_delete_trash() {
    // Dest has file, source is empty, delete_mode = Trash
    let src_tree = FileTree::new(PathBuf::from("/src"));

    let mut dest_tree = FileTree::new(PathBuf::from("/dest"));
    dest_tree.insert(
        PathBuf::from("old_file.txt"),
        create_test_entry("old_file.txt", 512, 1000),
    );

    let config = create_test_config(DeleteMode::Trash);
    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(plan.stats.delete_count, 1);
    assert_eq!(plan.actions.len(), 1);
    assert!(plan.actions[0].is_delete());
    if let SyncAction::Delete(path) = &plan.actions[0] {
        assert_eq!(path, &PathBuf::from("old_file.txt"));
    }
}

#[test]
fn test_diff_delete_none() {
    // Dest has file, source is empty, delete_mode = None
    let src_tree = FileTree::new(PathBuf::from("/src"));

    let mut dest_tree = FileTree::new(PathBuf::from("/dest"));
    dest_tree.insert(
        PathBuf::from("orphan.txt"),
        create_test_entry("orphan.txt", 256, 1000),
    );

    let config = create_test_config(DeleteMode::None);
    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    // Should not generate any delete actions
    assert_eq!(plan.stats.delete_count, 0);
    assert_eq!(plan.actions.len(), 0);
}

#[test]
fn test_diff_plan_stats() {
    // Complex scenario with multiple action types
    let mut src_tree = FileTree::new(PathBuf::from("/src"));
    src_tree.insert(
        PathBuf::from("new.txt"),
        create_test_entry("new.txt", 1000, 1000),
    );
    src_tree.insert(
        PathBuf::from("update.txt"),
        create_test_entry("update.txt", 2000, 2000), // Newer
    );
    src_tree.insert(
        PathBuf::from("same.txt"),
        create_test_entry("same.txt", 500, 1000),
    );

    let mut dest_tree = FileTree::new(PathBuf::from("/dest"));
    dest_tree.insert(
        PathBuf::from("update.txt"),
        create_test_entry("update.txt", 2000, 1000), // Older
    );
    dest_tree.insert(
        PathBuf::from("same.txt"),
        create_test_entry("same.txt", 500, 1000), // Identical
    );
    dest_tree.insert(
        PathBuf::from("old.txt"),
        create_test_entry("old.txt", 300, 1000),
    );

    let config = create_test_config(DeleteMode::Trash);
    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    // Verify counts
    assert_eq!(plan.stats.copy_count, 1); // new.txt
    assert_eq!(plan.stats.overwrite_count, 1); // update.txt
    assert_eq!(plan.stats.skip_count, 1); // same.txt
    assert_eq!(plan.stats.delete_count, 1); // old.txt

    // Verify transfer statistics (only CopyNew + Overwrite)
    assert_eq!(plan.stats.total_files, 2);
    assert_eq!(plan.stats.total_bytes, 3000); // 1000 + 2000
}

#[test]
fn test_diff_sorting_by_path() {
    // Verify actions are sorted by path
    let mut src_tree = FileTree::new(PathBuf::from("/src"));
    src_tree.insert(
        PathBuf::from("z_file.txt"),
        create_test_entry("z_file.txt", 100, 1000),
    );
    src_tree.insert(
        PathBuf::from("a_file.txt"),
        create_test_entry("a_file.txt", 200, 1000),
    );
    src_tree.insert(
        PathBuf::from("m_file.txt"),
        create_test_entry("m_file.txt", 300, 1000),
    );

    let dest_tree = FileTree::new(PathBuf::from("/dest"));
    let config = create_test_config(DeleteMode::None);

    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    // Actions should be sorted alphabetically
    assert_eq!(plan.actions.len(), 3);
    assert_eq!(plan.actions[0].path(), Some(&PathBuf::from("a_file.txt")));
    assert_eq!(plan.actions[1].path(), Some(&PathBuf::from("m_file.txt")));
    assert_eq!(plan.actions[2].path(), Some(&PathBuf::from("z_file.txt")));
}

#[test]
fn test_diff_delete_permanent() {
    // Test with DeleteMode::Permanent
    let src_tree = FileTree::new(PathBuf::from("/src"));

    let mut dest_tree = FileTree::new(PathBuf::from("/dest"));
    dest_tree.insert(
        PathBuf::from("doomed.txt"),
        create_test_entry("doomed.txt", 100, 1000),
    );

    let config = create_test_config(DeleteMode::Permanent);
    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(plan.stats.delete_count, 1);
    assert!(plan.actions[0].is_delete());
}

#[test]
fn test_diff_empty_trees() {
    // Both trees empty
    let src_tree = FileTree::new(PathBuf::from("/src"));
    let dest_tree = FileTree::new(PathBuf::from("/dest"));
    let config = create_test_config(DeleteMode::None);

    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(plan.actions.len(), 0);
    assert_eq!(plan.stats.total_files, 0);
    assert_eq!(plan.stats.total_bytes, 0);
}

#[test]
fn test_diff_nested_paths() {
    // Test with nested directory structures
    let mut src_tree = FileTree::new(PathBuf::from("/src"));
    src_tree.insert(
        PathBuf::from("dir/subdir/file.txt"),
        create_test_entry("dir/subdir/file.txt", 1024, 1000),
    );

    let dest_tree = FileTree::new(PathBuf::from("/dest"));
    let config = create_test_config(DeleteMode::None);

    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);

    assert_eq!(plan.stats.copy_count, 1);
    assert!(plan.actions[0].is_copy_new());
}
