//! Trash-based delete operations
//!
//! Deleted files are moved under `.kopy_trash/<timestamp>/` with original
//! relative paths preserved. A manifest is updated for recovery/audit.

use crate::executor::copy::copy_file_atomic;
use crate::types::KopyError;
use crate::Config;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::Path;

/// Represents a single deleted file in the trash
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedFile {
    /// Relative path from dest_root (original location)
    pub original_path: String,
    /// Relative path inside .kopy_trash
    pub trash_path: String,
    /// ISO 8601 timestamp when file was deleted
    pub deleted_at: String,
    /// File size in bytes
    pub size: u64,
}

/// Manifest file that tracks all deleted files in a trash snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashManifest {
    /// List of deleted files in this snapshot
    pub files: Vec<DeletedFile>,
}

impl TrashManifest {
    /// Create a new empty manifest
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// Add a deleted file entry to the manifest
    pub fn add_file(&mut self, file: DeletedFile) {
        self.files.push(file);
    }
}

impl Default for TrashManifest {
    fn default() -> Self {
        Self::new()
    }
}

/// Move a file to trash instead of permanently deleting
///
/// Files moved within the same second are grouped under one timestamp directory.
///
/// # Arguments
/// * `target_path` - Absolute path to file being deleted
/// * `dest_root` - Destination root directory (where .kopy_trash will be created)
/// * `relative_path` - Relative path from dest_root (for preserving structure)
/// * `config` - Configuration (used for copy_file_atomic if needed)
///
/// # Safety
/// - Original file is NEVER deleted if copy fails
/// - Uses atomic rename when possible
/// - Creates parent directories as needed
///
/// # Example
/// ```no_run
/// use kopy::executor::trash::move_to_trash;
/// use kopy::Config;
/// use std::path::Path;
///
/// let config = Config::default();
/// move_to_trash(
///     Path::new("/dest/subdir/file.txt"),
///     Path::new("/dest"),
///     Path::new("subdir/file.txt"),
///     &config
/// )?;
/// # Ok::<(), kopy::types::KopyError>(())
/// ```
pub fn move_to_trash(
    target_path: &Path,
    dest_root: &Path,
    relative_path: &Path,
    config: &Config,
) -> Result<(), KopyError> {
    let timestamp = Local::now().format("%Y-%m-%d_%H%M%S").to_string();

    let trash_root = dest_root.join(".kopy_trash").join(&timestamp);
    let trash_file_path = trash_root.join(relative_path);

    if let Some(parent) = trash_file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| map_file_error(parent, e))?;
    }

    let target_metadata =
        fs::symlink_metadata(target_path).map_err(|e| map_file_error(target_path, e))?;
    let file_size = target_metadata.len();

    match fs::rename(target_path, &trash_file_path) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            if target_metadata.file_type().is_symlink() {
                let target =
                    fs::read_link(target_path).map_err(|e| map_file_error(target_path, e))?;
                create_symlink(&target, &trash_file_path)
                    .map_err(|e| map_file_error(&trash_file_path, e))?;
            } else {
                copy_file_atomic(target_path, &trash_file_path, config)?;
            }
            fs::remove_file(target_path).map_err(|e| map_file_error(target_path, e))?;
        }
        Err(e) => return Err(map_file_error(target_path, e)),
    }

    let manifest_path = trash_root.join("MANIFEST.json");

    // Manifest writes use a read-modify-write flow and are not transactional.
    let mut manifest = if manifest_path.exists() {
        let manifest_content =
            fs::read_to_string(&manifest_path).map_err(|e| map_file_error(&manifest_path, e))?;
        serde_json::from_str(&manifest_content)
            .map_err(|e| KopyError::Validation(format!("Failed to parse MANIFEST.json: {}", e)))?
    } else {
        TrashManifest::new()
    };

    manifest.add_file(DeletedFile {
        original_path: relative_path.to_string_lossy().to_string(),
        trash_path: relative_path.to_string_lossy().to_string(),
        deleted_at: Local::now().to_rfc3339(),
        size: file_size,
    });

    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| KopyError::Validation(format!("Failed to serialize MANIFEST.json: {}", e)))?;

    fs::write(&manifest_path, manifest_json).map_err(|e| map_file_error(&manifest_path, e))?;

    Ok(())
}

fn map_file_error(path: &Path, error: Error) -> KopyError {
    if matches!(error.kind(), ErrorKind::PermissionDenied) {
        KopyError::PermissionDenied {
            path: path.to_path_buf(),
        }
    } else if matches!(error.kind(), ErrorKind::StorageFull)
        || matches!(error.raw_os_error(), Some(28 | 122))
    {
        KopyError::DiskFull {
            available: 0,
            needed: 1,
        }
    } else {
        KopyError::Io(error)
    }
}

#[cfg(unix)]
fn create_symlink(target: &Path, link_path: &Path) -> Result<(), Error> {
    std::os::unix::fs::symlink(target, link_path)
}

#[cfg(windows)]
fn create_symlink(target: &Path, link_path: &Path) -> Result<(), Error> {
    use std::os::windows::fs::{symlink_dir, symlink_file};
    match symlink_file(target, link_path) {
        Ok(()) => Ok(()),
        Err(file_err) => match symlink_dir(target, link_path) {
            Ok(()) => Ok(()),
            Err(_) => Err(file_err),
        },
    }
}
