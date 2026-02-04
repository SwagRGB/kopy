//! Trash-based delete operations

use std::path::Path;

/// Move a file to trash instead of permanently deleting
pub fn move_to_trash(
    _file_path: &Path,
    _dest_root: &Path,
) -> Result<(), crate::types::KopyError> {
    // TODO: Implement trash-based delete
    todo!("Implement move_to_trash")
}
