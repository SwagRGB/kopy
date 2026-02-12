//! SyncAction - Actions determined by the diff engine

use super::FileEntry;
use std::path::PathBuf;

/// Sync action determined by diff engine
#[derive(Debug, Clone, PartialEq)]
pub enum SyncAction {
    /// Copy new file (exists in src, missing in dest)
    CopyNew(FileEntry),

    /// Overwrite existing file (src and dest differ)
    Overwrite(FileEntry),

    /// Delete file (exists in dest, missing in src)
    Delete(PathBuf),

    /// Move/rename action.
    Move { from: PathBuf, to: PathBuf },

    /// Skip (files identical)
    Skip,
}

impl SyncAction {
    /// Check if this action is CopyNew
    pub fn is_copy_new(&self) -> bool {
        matches!(self, SyncAction::CopyNew(_))
    }

    /// Check if this action is Overwrite
    pub fn is_overwrite(&self) -> bool {
        matches!(self, SyncAction::Overwrite(_))
    }

    /// Check if this action is Delete
    pub fn is_delete(&self) -> bool {
        matches!(self, SyncAction::Delete(_))
    }

    /// Check if this action is Move.
    pub fn is_move(&self) -> bool {
        matches!(self, SyncAction::Move { .. })
    }

    /// Check if this action is Skip
    pub fn is_skip(&self) -> bool {
        matches!(self, SyncAction::Skip)
    }

    /// Check if this action requires file transfer
    ///
    /// Returns true for CopyNew and Overwrite, false otherwise
    pub fn requires_transfer(&self) -> bool {
        matches!(self, SyncAction::CopyNew(_) | SyncAction::Overwrite(_))
    }

    /// Get the path associated with this action
    ///
    /// Returns None for Skip variant
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            SyncAction::CopyNew(entry) | SyncAction::Overwrite(entry) => Some(&entry.path),
            SyncAction::Delete(path) => Some(path),
            SyncAction::Move { to, .. } => Some(to),
            SyncAction::Skip => None,
        }
    }

    /// Get the FileEntry if this action contains one
    ///
    /// Returns Some for CopyNew and Overwrite, None otherwise
    pub fn file_entry(&self) -> Option<&FileEntry> {
        match self {
            SyncAction::CopyNew(entry) | SyncAction::Overwrite(entry) => Some(entry),
            _ => None,
        }
    }

    /// Get a human-readable action name for UI display
    pub fn action_name(&self) -> &'static str {
        match self {
            SyncAction::CopyNew(_) => "Copy",
            SyncAction::Overwrite(_) => "Update",
            SyncAction::Delete(_) => "Delete",
            SyncAction::Move { .. } => "Move",
            SyncAction::Skip => "Skip",
        }
    }
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

impl DeleteMode {
    /// Check if this delete mode is safe (non-destructive)
    ///
    /// Returns true for None and Trash, false for Permanent
    pub fn is_safe(&self) -> bool {
        matches!(self, DeleteMode::None | DeleteMode::Trash)
    }

    /// Check if this delete mode is destructive
    ///
    /// Returns true for Permanent, false otherwise
    pub fn is_destructive(&self) -> bool {
        matches!(self, DeleteMode::Permanent)
    }

    /// Get a human-readable description of this delete mode
    pub fn description(&self) -> &'static str {
        match self {
            DeleteMode::None => "No deletions",
            DeleteMode::Trash => "Move to trash (recoverable)",
            DeleteMode::Permanent => "Permanent deletion (DANGEROUS)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn create_test_entry(name: &str, size: u64) -> FileEntry {
        FileEntry::new(
            PathBuf::from(name),
            size,
            UNIX_EPOCH + Duration::from_secs(1000),
            0o644,
        )
    }

    // SyncAction Tests

    #[test]
    fn test_copy_new_variant() {
        let entry = create_test_entry("file.txt", 1024);
        let action = SyncAction::CopyNew(entry.clone());

        assert!(action.is_copy_new());
        assert!(!action.is_overwrite());
        assert!(!action.is_delete());
        assert!(!action.is_move());
        assert!(!action.is_skip());
        assert!(action.requires_transfer());
        assert_eq!(action.action_name(), "Copy");
        assert_eq!(action.path(), Some(&PathBuf::from("file.txt")));
        assert_eq!(action.file_entry(), Some(&entry));
    }

    #[test]
    fn test_overwrite_variant() {
        let entry = create_test_entry("existing.txt", 2048);
        let action = SyncAction::Overwrite(entry.clone());

        assert!(!action.is_copy_new());
        assert!(action.is_overwrite());
        assert!(!action.is_delete());
        assert!(!action.is_move());
        assert!(!action.is_skip());
        assert!(action.requires_transfer());
        assert_eq!(action.action_name(), "Update");
        assert_eq!(action.path(), Some(&PathBuf::from("existing.txt")));
        assert_eq!(action.file_entry(), Some(&entry));
    }

    #[test]
    fn test_delete_variant() {
        let path = PathBuf::from("old_file.txt");
        let action = SyncAction::Delete(path.clone());

        assert!(!action.is_copy_new());
        assert!(!action.is_overwrite());
        assert!(action.is_delete());
        assert!(!action.is_move());
        assert!(!action.is_skip());
        assert!(!action.requires_transfer());
        assert_eq!(action.action_name(), "Delete");
        assert_eq!(action.path(), Some(&path));
        assert_eq!(action.file_entry(), None);
    }

    #[test]
    fn test_skip_variant() {
        let action = SyncAction::Skip;

        assert!(!action.is_copy_new());
        assert!(!action.is_overwrite());
        assert!(!action.is_delete());
        assert!(!action.is_move());
        assert!(action.is_skip());
        assert!(!action.requires_transfer());
        assert_eq!(action.action_name(), "Skip");
        assert_eq!(action.path(), None);
        assert_eq!(action.file_entry(), None);
    }

    #[test]
    fn test_move_variant() {
        let from = PathBuf::from("old_location.txt");
        let to = PathBuf::from("new_location.txt");
        let action = SyncAction::Move {
            from: from.clone(),
            to: to.clone(),
        };

        assert!(!action.is_copy_new());
        assert!(!action.is_overwrite());
        assert!(!action.is_delete());
        assert!(action.is_move());
        assert!(!action.is_skip());
        assert!(!action.requires_transfer());
        assert_eq!(action.action_name(), "Move");
        assert_eq!(action.path(), Some(&to));
        assert_eq!(action.file_entry(), None);
    }

    #[test]
    fn test_requires_transfer() {
        let entry = create_test_entry("file.txt", 100);

        assert!(SyncAction::CopyNew(entry.clone()).requires_transfer());
        assert!(SyncAction::Overwrite(entry).requires_transfer());
        assert!(!SyncAction::Delete(PathBuf::from("file.txt")).requires_transfer());
        assert!(!SyncAction::Skip.requires_transfer());
        assert!(!SyncAction::Move {
            from: PathBuf::from("a"),
            to: PathBuf::from("b")
        }
        .requires_transfer());
    }

    #[test]
    fn test_path_extraction() {
        let entry = create_test_entry("test.txt", 500);
        let path = PathBuf::from("delete.txt");

        assert_eq!(
            SyncAction::CopyNew(entry.clone()).path(),
            Some(&PathBuf::from("test.txt"))
        );
        assert_eq!(
            SyncAction::Overwrite(entry).path(),
            Some(&PathBuf::from("test.txt"))
        );
        assert_eq!(SyncAction::Delete(path.clone()).path(), Some(&path));
        assert_eq!(SyncAction::Skip.path(), None);
        assert_eq!(
            SyncAction::Move {
                from: PathBuf::from("old"),
                to: PathBuf::from("new")
            }
            .path(),
            Some(&PathBuf::from("new"))
        );
    }

    #[test]
    fn test_file_entry_extraction() {
        let entry = create_test_entry("file.txt", 1024);

        assert_eq!(
            SyncAction::CopyNew(entry.clone()).file_entry(),
            Some(&entry)
        );
        assert_eq!(
            SyncAction::Overwrite(entry.clone()).file_entry(),
            Some(&entry)
        );
        assert_eq!(
            SyncAction::Delete(PathBuf::from("file.txt")).file_entry(),
            None
        );
        assert_eq!(SyncAction::Skip.file_entry(), None);
        assert_eq!(
            SyncAction::Move {
                from: PathBuf::from("a"),
                to: PathBuf::from("b")
            }
            .file_entry(),
            None
        );
    }

    #[test]
    fn test_action_name() {
        let entry = create_test_entry("file.txt", 100);

        assert_eq!(SyncAction::CopyNew(entry.clone()).action_name(), "Copy");
        assert_eq!(SyncAction::Overwrite(entry).action_name(), "Update");
        assert_eq!(
            SyncAction::Delete(PathBuf::from("file.txt")).action_name(),
            "Delete"
        );
        assert_eq!(SyncAction::Skip.action_name(), "Skip");
        assert_eq!(
            SyncAction::Move {
                from: PathBuf::from("a"),
                to: PathBuf::from("b")
            }
            .action_name(),
            "Move"
        );
    }

    #[test]
    fn test_pattern_matching() {
        let entry = create_test_entry("file.txt", 100);
        let actions = vec![
            SyncAction::CopyNew(entry.clone()),
            SyncAction::Overwrite(entry),
            SyncAction::Delete(PathBuf::from("old.txt")),
            SyncAction::Skip,
            SyncAction::Move {
                from: PathBuf::from("a"),
                to: PathBuf::from("b"),
            },
        ];

        let mut copy_count = 0;
        let mut overwrite_count = 0;
        let mut delete_count = 0;
        let mut skip_count = 0;
        let mut move_count = 0;

        for action in actions {
            match action {
                SyncAction::CopyNew(_) => copy_count += 1,
                SyncAction::Overwrite(_) => overwrite_count += 1,
                SyncAction::Delete(_) => delete_count += 1,
                SyncAction::Skip => skip_count += 1,
                SyncAction::Move { .. } => move_count += 1,
            }
        }

        assert_eq!(copy_count, 1);
        assert_eq!(overwrite_count, 1);
        assert_eq!(delete_count, 1);
        assert_eq!(skip_count, 1);
        assert_eq!(move_count, 1);
    }

    #[test]
    fn test_sync_action_equality() {
        let entry1 = create_test_entry("file.txt", 100);
        let entry2 = create_test_entry("file.txt", 100);
        let entry3 = create_test_entry("other.txt", 200);

        assert_eq!(
            SyncAction::CopyNew(entry1.clone()),
            SyncAction::CopyNew(entry2)
        );
        assert_ne!(SyncAction::CopyNew(entry1), SyncAction::CopyNew(entry3));
        assert_eq!(SyncAction::Skip, SyncAction::Skip);
        assert_ne!(
            SyncAction::Delete(PathBuf::from("a.txt")),
            SyncAction::Delete(PathBuf::from("b.txt"))
        );
    }

    // DeleteMode Tests

    #[test]
    fn test_delete_mode_default() {
        let mode: DeleteMode = Default::default();
        assert_eq!(mode, DeleteMode::None);
    }

    #[test]
    fn test_delete_mode_safety() {
        assert!(DeleteMode::None.is_safe());
        assert!(DeleteMode::Trash.is_safe());
        assert!(!DeleteMode::Permanent.is_safe());

        assert!(!DeleteMode::None.is_destructive());
        assert!(!DeleteMode::Trash.is_destructive());
        assert!(DeleteMode::Permanent.is_destructive());
    }

    #[test]
    fn test_delete_mode_description() {
        assert_eq!(DeleteMode::None.description(), "No deletions");
        assert_eq!(
            DeleteMode::Trash.description(),
            "Move to trash (recoverable)"
        );
        assert_eq!(
            DeleteMode::Permanent.description(),
            "Permanent deletion (DANGEROUS)"
        );
    }

    #[test]
    fn test_delete_mode_equality() {
        assert_eq!(DeleteMode::None, DeleteMode::None);
        assert_eq!(DeleteMode::Trash, DeleteMode::Trash);
        assert_eq!(DeleteMode::Permanent, DeleteMode::Permanent);

        assert_ne!(DeleteMode::None, DeleteMode::Trash);
        assert_ne!(DeleteMode::Trash, DeleteMode::Permanent);
        assert_ne!(DeleteMode::None, DeleteMode::Permanent);
    }

    #[test]
    fn test_delete_mode_copy() {
        let mode1 = DeleteMode::Trash;
        let mode2 = mode1; // Copy trait

        assert_eq!(mode1, mode2);
        assert_eq!(mode1, DeleteMode::Trash);
    }
}
