//! SyncAction plan generation

use crate::diff::{compare_files, DiffPlan};
use crate::types::{DeleteMode, FileTree, SyncAction};
use crate::Config;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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
    let mut planned_deletes: HashSet<PathBuf> = HashSet::new();
    let dest_parent_prefixes = build_dest_parent_prefixes(dest_tree);
    let allow_deletes = config.delete_mode != DeleteMode::None;

    for (path, src_entry) in src_tree.iter() {
        if allow_deletes {
            for conflict_path in conflict_delete_roots(path, dest_tree, &dest_parent_prefixes) {
                if planned_deletes.insert(conflict_path.clone()) {
                    plan.add_action(SyncAction::Delete(conflict_path));
                }
            }
        }

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

    if allow_deletes {
        for (path, _dest_entry) in dest_tree.iter() {
            if !src_tree.contains(path)
                && !planned_deletes.contains(path)
                && !is_covered_by_planned_delete(path, &planned_deletes)
            {
                plan.add_action(SyncAction::Delete(path.clone()));
            }
        }
    }

    plan.sort_by_path();

    plan
}

fn build_dest_parent_prefixes(dest_tree: &FileTree) -> HashSet<PathBuf> {
    let mut prefixes = HashSet::new();
    for dest_path in dest_tree.paths() {
        for ancestor in dest_path.ancestors().skip(1) {
            if ancestor.as_os_str().is_empty() {
                continue;
            }
            prefixes.insert(ancestor.to_path_buf());
        }
    }
    prefixes
}

fn conflict_delete_roots(
    src_path: &Path,
    dest_tree: &FileTree,
    dest_parent_prefixes: &HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    // Source has nested path but destination has a blocking file at an ancestor.
    for ancestor in src_path.ancestors().skip(1) {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let ancestor_buf = ancestor.to_path_buf();
        if dest_tree.contains(&ancestor_buf) {
            roots.push(ancestor_buf);
            break;
        }
    }

    // Source has a file path, destination has entries beneath the same path (directory conflict).
    if dest_parent_prefixes.contains(src_path) {
        roots.push(src_path.to_path_buf());
    }

    roots
}

fn is_covered_by_planned_delete(path: &Path, planned_deletes: &HashSet<PathBuf>) -> bool {
    path.ancestors().any(|ancestor| {
        !ancestor.as_os_str().is_empty() && planned_deletes.contains(&ancestor.to_path_buf())
    })
}
