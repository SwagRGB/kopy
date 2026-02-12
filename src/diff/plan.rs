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
