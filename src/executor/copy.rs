//! Atomic file copy operations

use std::path::Path;

/// Copy a file atomically using .part files
pub fn copy_file_atomic(
    _src_path: &Path,
    _dest_path: &Path,
    _expected_size: u64,
) -> Result<(), crate::types::KopyError> {
    // TODO: Implement atomic file copy
    todo!("Implement copy_file_atomic")
}
