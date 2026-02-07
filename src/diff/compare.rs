//! File comparison logic

use crate::types::{FileEntry, SyncAction};
use crate::Config;

/// Compare two files and determine what action is needed
///
/// This implements Tier 1 metadata-based comparison (Algorithm 2 from implementation_plan.md):
///
/// 1. **Size mismatch**: If sizes differ, files are definitely different → Overwrite
/// 2. **Modification time comparison**:
///    - Source newer (src.mtime > dest.mtime) → Overwrite
///    - Dest newer (src.mtime < dest.mtime) → Skip (Phase 1: avoid conflicts)
///    - Same mtime → Skip (files are identical)
///
/// # Phase 1 Constraint
/// This does NOT implement content hashing (that's Phase 4.2).
/// Comparison is purely metadata-based.
///
/// # Arguments
/// * `src` - Source file entry
/// * `dest` - Destination file entry
/// * `config` - Configuration (for future checksum mode support)
///
/// # Returns
/// The appropriate `SyncAction` based on the comparison
pub fn compare_files(src: &FileEntry, dest: &FileEntry, _config: &Config) -> SyncAction {
    // ═══════════════════════════════════════════════════════════
    // TIER 1: Metadata-based comparison (instant, no I/O)
    // ═══════════════════════════════════════════════════════════

    // Size mismatch = definitely different
    if src.size != dest.size {
        return SyncAction::Overwrite(src.clone());
    }

    // Modification time comparison
    match src.mtime.cmp(&dest.mtime) {
        std::cmp::Ordering::Greater => {
            // Source is newer → update needed
            SyncAction::Overwrite(src.clone())
        }
        std::cmp::Ordering::Less => {
            // Destination is newer → CONFLICT!
            // Phase 1: Skip conflicts (don't overwrite newer files)
            // TODO: In Phase 2, emit a Conflict event for user resolution
            SyncAction::Skip
        }
        std::cmp::Ordering::Equal => {
            // Same size and mtime → files are identical
            SyncAction::Skip
        }
    }

    // Note: Tier 2 (content hashing) will be implemented in Phase 4.2
}
