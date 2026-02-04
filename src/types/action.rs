//! SyncAction - Actions determined by the diff engine

use super::FileEntry;
use std::path::PathBuf;

/// Sync action determined by diff engine
#[derive(Debug, Clone)]
pub enum SyncAction {
    /// Copy new file (exists in src, missing in dest)
    CopyNew(FileEntry),

    /// Overwrite existing file (src and dest differ)
    Overwrite(FileEntry),

    /// Delete file (exists in dest, missing in src)
    Delete(PathBuf),

    /// Move/rename detection (Phase 3 - optional optimization)
    Move { from: PathBuf, to: PathBuf },

    /// Skip (files identical)
    Skip,
}

/// Delete behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeleteMode {
    /// Don't delete anything
    #[default]
    None,

    /// Move to .kopy_trash/
    Trash,

    /// Permanent deletion (dangerous)
    Permanent,
}
