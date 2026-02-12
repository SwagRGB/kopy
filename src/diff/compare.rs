//! File comparison logic

use crate::hash::compute_hash;
use crate::types::{FileEntry, SyncAction};
use crate::Config;

/// Compare two files and determine what action is needed
///
/// This implements cascading comparison (Algorithm 2 from implementation_plan.md):
///
/// **Priority Order:**
/// 1. **Size mismatch**: If sizes differ → Overwrite (cheap, always check first)
/// 2. **Checksum mode enabled**:
///    - Compute Blake3 hashes for both files
///    - If hashes differ → Overwrite
///    - If hashes match → Skip
/// 3. **Checksum mode disabled** (Tier 1 - metadata only):
///    - Source newer (mtime > dest.mtime) → Overwrite
///    - Dest newer (mtime < src.mtime) → Skip (Phase 1: avoid conflicts)
///    - Same mtime → Skip
///
/// # Arguments
/// * `src` - Source file entry
/// * `dest` - Destination file entry
/// * `config` - Configuration (includes checksum_mode flag)
///
/// # Returns
/// The appropriate `SyncAction` based on the comparison
pub fn compare_files(src: &FileEntry, dest: &FileEntry, config: &Config) -> SyncAction {
    // PRIORITY 1: Size check (cheap, always do this first)
    if src.size != dest.size {
        return SyncAction::Overwrite(src.clone());
    }

    // PRIORITY 2: Checksum mode (Tier 2 - content hashing)
    if config.checksum_mode {
        // Compute full paths for hashing
        let src_path = config.source.join(&src.path);
        let dest_path = config.destination.join(&dest.path);

        // Compute hashes (or use cached if available)
        let src_hash = match src.hash {
            Some(hash) => hash,
            None => match compute_hash(&src_path) {
                Ok(hash) => hash,
                Err(_) => {
                    // If we can't hash source, fall back to overwrite
                    // (better to copy than skip a potentially different file)
                    return SyncAction::Overwrite(src.clone());
                }
            },
        };

        let dest_hash = match dest.hash {
            Some(hash) => hash,
            None => match compute_hash(&dest_path) {
                Ok(hash) => hash,
                Err(_) => {
                    // If we can't hash dest, assume it's different
                    return SyncAction::Overwrite(src.clone());
                }
            },
        };

        // Compare hashes
        if src_hash != dest_hash {
            return SyncAction::Overwrite(src.clone());
        } else {
            return SyncAction::Skip;
        }
    }

    // PRIORITY 3: Metadata comparison (Tier 1 - legacy mode)
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
}
