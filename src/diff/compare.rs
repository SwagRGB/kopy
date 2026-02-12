//! File comparison logic

use crate::hash::compute_hash;
use crate::types::{FileEntry, SyncAction};
use crate::Config;

/// Compare two files and determine what action is needed
///
/// The decision order is:
/// 1. File kind differences (symlink vs regular, or symlink target mismatch)
/// 2. Size differences
/// 3. Optional content hash comparison (`--checksum`)
/// 4. Metadata fallback (mtime)
///
/// # Arguments
/// * `src` - Source file entry
/// * `dest` - Destination file entry
/// * `config` - Configuration (includes checksum_mode flag)
///
/// # Returns
/// The appropriate `SyncAction` based on the comparison
pub fn compare_files(src: &FileEntry, dest: &FileEntry, config: &Config) -> SyncAction {
    if src.is_symlink != dest.is_symlink {
        return SyncAction::Overwrite(src.clone());
    }

    if src.is_symlink && dest.is_symlink {
        return if src.symlink_target == dest.symlink_target {
            SyncAction::Skip
        } else {
            SyncAction::Overwrite(src.clone())
        };
    }

    if src.size != dest.size {
        return SyncAction::Overwrite(src.clone());
    }

    if config.checksum_mode {
        let src_path = config.source.join(&src.path);
        let dest_path = config.destination.join(&dest.path);

        let src_hash = match src.hash {
            Some(hash) => hash,
            None => match compute_hash(&src_path) {
                Ok(hash) => hash,
                Err(_) => {
                    return SyncAction::Overwrite(src.clone());
                }
            },
        };

        let dest_hash = match dest.hash {
            Some(hash) => hash,
            None => match compute_hash(&dest_path) {
                Ok(hash) => hash,
                Err(_) => {
                    return SyncAction::Overwrite(src.clone());
                }
            },
        };

        if src_hash != dest_hash {
            return SyncAction::Overwrite(src.clone());
        } else {
            return SyncAction::Skip;
        }
    }

    match src.mtime.cmp(&dest.mtime) {
        std::cmp::Ordering::Greater => SyncAction::Overwrite(src.clone()),
        std::cmp::Ordering::Less => SyncAction::Skip,
        std::cmp::Ordering::Equal => SyncAction::Skip,
    }
}
