//! Diff engine types and plan generation

use crate::types::SyncAction;

/// Diff plan containing actions and statistics
#[derive(Debug, Clone, PartialEq)]
pub struct DiffPlan {
    /// List of sync actions to execute
    pub actions: Vec<SyncAction>,

    /// Aggregate statistics about the plan
    pub stats: PlanStats,
}

impl DiffPlan {
    /// Create a new empty diff plan
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            stats: PlanStats::default(),
        }
    }

    /// Add an action to the plan and update statistics
    pub fn add_action(&mut self, action: SyncAction) {
        // Update statistics based on action type
        match &action {
            SyncAction::CopyNew(entry) => {
                self.stats.copy_count += 1;
                self.stats.total_files += 1;
                self.stats.total_bytes += entry.size;
            }
            SyncAction::Overwrite(entry) => {
                self.stats.overwrite_count += 1;
                self.stats.total_files += 1;
                self.stats.total_bytes += entry.size;
            }
            SyncAction::Delete(_) => {
                self.stats.delete_count += 1;
            }
            SyncAction::Skip => {
                self.stats.skip_count += 1;
            }
            SyncAction::Move { .. } => {
                // Phase 3 feature - not counted in Phase 1
            }
        }

        self.actions.push(action);
    }

    /// Sort actions by path for deterministic output
    pub fn sort_by_path(&mut self) {
        self.actions.sort_by(|a, b| {
            let path_a = a.path();
            let path_b = b.path();

            match (path_a, path_b) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });
    }
}

impl Default for DiffPlan {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about a diff plan
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlanStats {
    /// Total number of files to transfer (CopyNew + Overwrite)
    pub total_files: usize,

    /// Total bytes to transfer (CopyNew + Overwrite)
    pub total_bytes: u64,

    /// Number of CopyNew actions
    pub copy_count: usize,

    /// Number of Overwrite actions
    pub overwrite_count: usize,

    /// Number of Delete actions
    pub delete_count: usize,

    /// Number of Skip actions
    pub skip_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FileEntry;
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    fn create_test_entry(name: &str, size: u64) -> FileEntry {
        FileEntry::new(
            PathBuf::from(name),
            size,
            UNIX_EPOCH + Duration::from_secs(1000),
            0o644,
        )
    }

    #[test]
    fn test_new_plan() {
        let plan = DiffPlan::new();
        assert!(plan.actions.is_empty());
        assert_eq!(plan.stats.total_files, 0);
        assert_eq!(plan.stats.total_bytes, 0);
    }

    #[test]
    fn test_add_copy_new_action() {
        let mut plan = DiffPlan::new();
        let entry = create_test_entry("file.txt", 1024);

        plan.add_action(SyncAction::CopyNew(entry));

        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.stats.copy_count, 1);
        assert_eq!(plan.stats.total_files, 1);
        assert_eq!(plan.stats.total_bytes, 1024);
    }

    #[test]
    fn test_add_overwrite_action() {
        let mut plan = DiffPlan::new();
        let entry = create_test_entry("file.txt", 2048);

        plan.add_action(SyncAction::Overwrite(entry));

        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.stats.overwrite_count, 1);
        assert_eq!(plan.stats.total_files, 1);
        assert_eq!(plan.stats.total_bytes, 2048);
    }

    #[test]
    fn test_add_delete_action() {
        let mut plan = DiffPlan::new();

        plan.add_action(SyncAction::Delete(PathBuf::from("old.txt")));

        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.stats.delete_count, 1);
        assert_eq!(plan.stats.total_files, 0); // Deletes don't count as transfers
        assert_eq!(plan.stats.total_bytes, 0);
    }

    #[test]
    fn test_add_skip_action() {
        let mut plan = DiffPlan::new();

        plan.add_action(SyncAction::Skip);

        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.stats.skip_count, 1);
        assert_eq!(plan.stats.total_files, 0);
        assert_eq!(plan.stats.total_bytes, 0);
    }

    #[test]
    fn test_sort_by_path() {
        let mut plan = DiffPlan::new();

        plan.add_action(SyncAction::CopyNew(create_test_entry("z.txt", 100)));
        plan.add_action(SyncAction::CopyNew(create_test_entry("a.txt", 200)));
        plan.add_action(SyncAction::CopyNew(create_test_entry("m.txt", 300)));

        plan.sort_by_path();

        assert_eq!(plan.actions[0].path(), Some(&PathBuf::from("a.txt")));
        assert_eq!(plan.actions[1].path(), Some(&PathBuf::from("m.txt")));
        assert_eq!(plan.actions[2].path(), Some(&PathBuf::from("z.txt")));
    }

    #[test]
    fn test_mixed_actions_stats() {
        let mut plan = DiffPlan::new();

        plan.add_action(SyncAction::CopyNew(create_test_entry("new.txt", 1000)));
        plan.add_action(SyncAction::Overwrite(create_test_entry("update.txt", 2000)));
        plan.add_action(SyncAction::Delete(PathBuf::from("old.txt")));
        plan.add_action(SyncAction::Skip);

        assert_eq!(plan.stats.copy_count, 1);
        assert_eq!(plan.stats.overwrite_count, 1);
        assert_eq!(plan.stats.delete_count, 1);
        assert_eq!(plan.stats.skip_count, 1);
        assert_eq!(plan.stats.total_files, 2); // Only CopyNew + Overwrite
        assert_eq!(plan.stats.total_bytes, 3000);
    }
}
