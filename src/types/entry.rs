//! FileEntry - Represents a single file in the sync tree

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// Represents a file in the sync tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative path from sync root
    pub path: PathBuf,

    /// File size in bytes
    pub size: u64,

    /// Last modification time (UTC)
    pub mtime: SystemTime,

    /// Unix permissions (mode bits)
    pub permissions: u32,

    /// Blake3 content hash (computed lazily)
    pub hash: Option<[u8; 32]>,

    /// Symlink metadata
    pub is_symlink: bool,
    pub symlink_target: Option<PathBuf>,
}
