//! SyncAction plan generation

use crate::diff::{compare_files, DiffPlan};
use crate::types::{DeleteMode, FileTree, SyncAction};
use crate::Config;

/// Generate a sync plan by comparing source and destination trees
///
/// The plan includes copy/update/skip actions for source entries and optional delete
/// actions for destination orphans when deletes are enabled.
///
/// # Arguments
/// * `src_tree` - Source directory tree
/// * `dest_tree` - Destination directory tree
/// * `config` - Configuration (includes delete_mode)
///
/// # Returns
/// A `DiffPlan` containing all sync actions and statistics
///
/// # Example
/// ```
/// use kopy::diff::generate_sync_plan;
/// use kopy::types::{FileEntry, FileTree};
/// use kopy::Config;
/// use std::path::PathBuf;
/// use std::time::{Duration, UNIX_EPOCH};
///
/// let mut src = FileTree::new(PathBuf::from("src"));
/// let dest = FileTree::new(PathBuf::from("dst"));
/// src.insert(
///     PathBuf::from("new.txt"),
///     FileEntry::new(
///         PathBuf::from("new.txt"),
///         4,
///         UNIX_EPOCH + Duration::from_secs(1_000),
///         0o644,
///     ),
/// );
///
/// let plan = generate_sync_plan(&src, &dest, &Config::default());
/// assert_eq!(plan.stats.copy_count, 1);
/// ```
pub fn generate_sync_plan(src_tree: &FileTree, dest_tree: &FileTree, config: &Config) -> DiffPlan {
    let mut plan = DiffPlan::new();

    for (path, src_entry) in src_tree.iter() {
        match dest_tree.get(path) {
            None => {
                plan.add_action(SyncAction::CopyNew(src_entry.clone()));
            }
            Some(dest_entry) => {
                let action = compare_files(src_entry, dest_entry, config);
                if !action.is_skip() {
                    plan.add_action(action);
                } else {
                    plan.add_action(SyncAction::Skip);
                }
            }
        }
    }

    if config.delete_mode != DeleteMode::None {
        for (path, _dest_entry) in dest_tree.iter() {
            if !src_tree.contains(path) {
                plan.add_action(SyncAction::Delete(path.clone()));
            }
        }
    }

    plan.sort_by_path();

    plan
}
