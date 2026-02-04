//! File comparison logic

use crate::types::{FileEntry, SyncAction};
use crate::Config;

/// Compare two files and determine what action is needed
pub fn compare_files(_src: &FileEntry, _dest: &FileEntry, _config: &Config) -> SyncAction {
    // TODO: Implement cascading file comparison
    todo!("Implement compare_files")
}
