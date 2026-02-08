//! Trash-based delete operations
//!
//! Implements Algorithm 4 from implementation_plan.md:
//! - Move files to `.kopy_trash/<TIMESTAMP>/` preserving relative paths
//! - Atomic moves when possible, with copy+delete fallback for cross-device
//! - Log metadata in MANIFEST.json for audit trails

use crate::executor::copy::copy_file_atomic;
use crate::types::KopyError;
use crate::Config;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

// ═══════════════════════════════════════════════════════════
// Data Structures
// ═══════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════
// Trash Operations
// ═══════════════════════════════════════════════════════════

/// Move a file to trash instead of permanently deleting
///
/// This implements Algorithm 4 from implementation_plan.md:
/// 1. Generate timestamp for trash snapshot directory
/// 2. Resolve trash paths preserving relative structure
/// 3. Attempt atomic rename (fast, single syscall)
/// 4. If cross-device error, fallback to copy+delete
/// 5. Log metadata to MANIFEST.json
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
    // ═══════════════════════════════════════════════════════════
    // STEP 1: Generate timestamp
    // ═══════════════════════════════════════════════════════════
    let timestamp = Local::now().format("%Y-%m-%d_%H%M%S").to_string();

    // ═══════════════════════════════════════════════════════════
    // STEP 2: Resolve paths
    // ═══════════════════════════════════════════════════════════
    let trash_root = dest_root.join(".kopy_trash").join(&timestamp);
    let trash_file_path = trash_root.join(relative_path);

    // ═══════════════════════════════════════════════════════════
    // STEP 3: Prepare - Create parent directories
    // ═══════════════════════════════════════════════════════════
    if let Some(parent) = trash_file_path.parent() {
        fs::create_dir_all(parent).map_err(KopyError::Io)?;
    }

    // Get file size before moving (for manifest)
    let file_size = fs::metadata(target_path).map_err(KopyError::Io)?.len();

    // ═══════════════════════════════════════════════════════════
    // STEP 4: Atomic Move with Fallback
    // ═══════════════════════════════════════════════════════════
    // Try atomic rename first (fast, single syscall)
    match fs::rename(target_path, &trash_file_path) {
        Ok(()) => {
            // Success - atomic rename worked
        }
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            // Fallback for cross-device moves:
            // 1. Copy to trash using atomic copy
            // 2. Only delete original if copy succeeded
            // 3. If copy fails, return error (original untouched)

            copy_file_atomic(target_path, &trash_file_path, config)?;

            // Copy succeeded - now safe to delete original
            fs::remove_file(target_path).map_err(KopyError::Io)?;
        }
        Err(e) => {
            // Other errors - propagate up
            return Err(KopyError::Io(e));
        }
    }

    // ═══════════════════════════════════════════════════════════
    // STEP 5: Log Metadata to MANIFEST.json
    // ═══════════════════════════════════════════════════════════
    let manifest_path = trash_root.join("MANIFEST.json");

    // Load existing manifest or create new one
    let mut manifest = if manifest_path.exists() {
        let manifest_content = fs::read_to_string(&manifest_path).map_err(KopyError::Io)?;
        serde_json::from_str(&manifest_content)
            .map_err(|e| KopyError::Validation(format!("Failed to parse MANIFEST.json: {}", e)))?
    } else {
        TrashManifest::new()
    };

    // Add this deleted file
    manifest.add_file(DeletedFile {
        original_path: relative_path.to_string_lossy().to_string(),
        trash_path: relative_path.to_string_lossy().to_string(),
        deleted_at: Local::now().to_rfc3339(), // ISO 8601 format
        size: file_size,
    });

    // Write manifest back to disk
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| KopyError::Validation(format!("Failed to serialize MANIFEST.json: {}", e)))?;

    fs::write(&manifest_path, manifest_json).map_err(KopyError::Io)?;

    Ok(())
}
