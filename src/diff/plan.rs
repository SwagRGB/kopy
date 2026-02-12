//! SyncAction plan generation

use crate::diff::{compare_files, DiffPlan};
use crate::types::{DeleteMode, FileTree, SyncAction};
use crate::Config;

/// Generate a sync plan by comparing source and destination trees
///
/// This implements Algorithm 2 from implementation_plan.md:
///
/// **Phase 1: Process files in source**
/// - For each file in source tree:
///   - If missing in dest → CopyNew
///   - If present in dest → compare_files() to determine action
///
/// **Phase 2: Process files only in destination (delete detection)**
/// - For each file in dest tree:
///   - If missing in source (orphan):
///     - Check delete_mode:
///       - None → skip
///       - Trash or Permanent → Delete
///
/// **Phase 3: Sorting**
/// - Sort all actions by path for deterministic output
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

    // PHASE 1: Process files in source
    for (path, src_entry) in src_tree.iter() {
        match dest_tree.get(path) {
            // ───────────────────────────────────────────────
            // Case 1: File doesn't exist in destination
            // ───────────────────────────────────────────────
            None => {
                plan.add_action(SyncAction::CopyNew(src_entry.clone()));
            }

            // ───────────────────────────────────────────────
            // Case 2: File exists in both locations
            // ───────────────────────────────────────────────
            Some(dest_entry) => {
                let action = compare_files(src_entry, dest_entry, config);
                // Only add non-Skip actions to keep plan concise
                if !action.is_skip() {
                    plan.add_action(action);
                } else {
                    // Still track skips in statistics
                    plan.add_action(SyncAction::Skip);
                }
            }
        }
    }

    // PHASE 2: Process files only in destination (deletes)
    if config.delete_mode != DeleteMode::None {
        for (path, _dest_entry) in dest_tree.iter() {
            // Check if this file exists in source
            if !src_tree.contains(path) {
                // Orphan file: exists in dest but not in source
                plan.add_action(SyncAction::Delete(path.clone()));
            }
        }
    }

    // PHASE 3: Sorting for deterministic output
    plan.sort_by_path();

    plan
}
