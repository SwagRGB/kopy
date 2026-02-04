//! SyncAction plan generation

use crate::types::{FileTree, SyncAction};
use crate::Config;

/// Generate a sync plan by comparing source and destination trees
pub fn generate_sync_plan(
    _src_tree: &FileTree,
    _dest_tree: &FileTree,
    _config: &Config,
) -> Vec<SyncAction> {
    // TODO: Implement sync plan generation
    todo!("Implement generate_sync_plan")
}
