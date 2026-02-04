//! Sequential directory walker (Phase 1)

use crate::types::{FileTree, KopyError};
use std::path::Path;

/// Scan a directory and build a FileTree
pub fn scan_directory(_root_path: &Path) -> Result<FileTree, KopyError> {
    // TODO: Implement directory scanning with walkdir
    todo!("Implement scan_directory")
}
