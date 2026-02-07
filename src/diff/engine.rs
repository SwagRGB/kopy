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

impl PlanStats {
    /// Estimate time remaining for the sync operation
    ///
    /// This provides a rough estimate based on the total bytes to transfer
    /// and an assumed transfer speed. The actual time will vary based on:
    /// - Disk I/O performance
    /// - File system overhead
    /// - Number of small vs large files
    /// - Network latency (for remote syncs)
    ///
    /// # Arguments
    /// * `bytes_per_second` - Assumed transfer speed in bytes/second
    ///   - Default recommendation: 100 MB/s for local SSD
    ///   - Typical values: 50-200 MB/s (local), 1-10 MB/s (network)
    ///
    /// # Returns
    /// Estimated duration in seconds
    ///
    /// # Example
    /// ```
    /// use kopy::diff::PlanStats;
    ///
    /// let stats = PlanStats {
    ///     total_bytes: 1_000_000_000, // 1 GB
    ///     total_files: 100,
    ///     ..Default::default()
    /// };
    ///
    /// // Estimate at 100 MB/s
    /// let seconds = stats.estimate_duration(100 * 1024 * 1024);
    /// assert_eq!(seconds, 10); // ~10 seconds
    /// ```
    pub fn estimate_duration(&self, bytes_per_second: u64) -> u64 {
        if bytes_per_second == 0 || self.total_bytes == 0 {
            return 0;
        }

        // Basic calculation: total_bytes / bytes_per_second
        let base_seconds = self.total_bytes / bytes_per_second;

        // Add overhead for file operations (open, close, metadata)
        // Estimate ~10ms per file for filesystem overhead
        let file_overhead_ms = self.total_files as u64 * 10;
        let file_overhead_seconds = file_overhead_ms / 1000;

        base_seconds + file_overhead_seconds
    }

    /// Estimate duration with a human-readable format
    ///
    /// # Arguments
    /// * `bytes_per_second` - Transfer speed in bytes/second
    ///
    /// # Returns
    /// Formatted string like "2m 30s" or "1h 15m"
    ///
    /// # Example
    /// ```
    /// use kopy::diff::PlanStats;
    ///
    /// let stats = PlanStats {
    ///     total_bytes: 500_000_000, // 500 MB
    ///     total_files: 50,
    ///     ..Default::default()
    /// };
    ///
    /// let estimate = stats.estimate_duration_human(50 * 1024 * 1024);
    /// // Returns something like "10s" or "1m 5s"
    /// ```
    pub fn estimate_duration_human(&self, bytes_per_second: u64) -> String {
        let total_seconds = self.estimate_duration(bytes_per_second);

        if total_seconds == 0 {
            return "0s".to_string();
        }

        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;

        if hours > 0 {
            if minutes > 0 {
                format!("{}h {}m", hours, minutes)
            } else {
                format!("{}h", hours)
            }
        } else if minutes > 0 {
            if seconds > 0 {
                format!("{}m {}s", minutes, seconds)
            } else {
                format!("{}m", minutes)
            }
        } else {
            format!("{}s", seconds)
        }
    }
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

    // ═══════════════════════════════════════════════════════════
    // Time Estimation Tests
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_estimate_duration_basic() {
        let stats = PlanStats {
            total_bytes: 1_000_000_000, // 1 GB
            total_files: 100,
            ..Default::default()
        };

        // At 100 MB/s
        let bytes_per_sec = 100 * 1024 * 1024;
        let duration = stats.estimate_duration(bytes_per_sec);

        // 1 GB / 100 MB/s = 9 seconds (integer division)
        // 100 files * 10ms = 1 second overhead
        assert_eq!(duration, 10); // 9 + 1
    }

    #[test]
    fn test_estimate_duration_with_file_overhead() {
        let stats = PlanStats {
            total_bytes: 10_000, // 10 KB (very small)
            total_files: 1000,   // Many small files
            ..Default::default()
        };

        // At 10 MB/s
        let bytes_per_sec = 10 * 1024 * 1024;
        let duration = stats.estimate_duration(bytes_per_sec);

        // 10 KB / 10 MB/s = ~0 seconds
        // 1000 files * 10ms = 10 seconds overhead
        assert_eq!(duration, 10); // File overhead dominates
    }

    #[test]
    fn test_estimate_duration_zero_bytes() {
        let stats = PlanStats {
            total_bytes: 0,
            total_files: 0,
            ..Default::default()
        };

        let duration = stats.estimate_duration(100 * 1024 * 1024);
        assert_eq!(duration, 0);
    }

    #[test]
    fn test_estimate_duration_zero_speed() {
        let stats = PlanStats {
            total_bytes: 1_000_000,
            total_files: 10,
            ..Default::default()
        };

        let duration = stats.estimate_duration(0);
        assert_eq!(duration, 0); // Avoid division by zero
    }

    #[test]
    fn test_estimate_duration_human_seconds() {
        let stats = PlanStats {
            total_bytes: 5_000_000, // 5 MB
            total_files: 10,
            ..Default::default()
        };

        // At 1 MB/s
        let estimate = stats.estimate_duration_human(1024 * 1024);
        assert_eq!(estimate, "4s"); // 4 seconds (integer division)
    }

    #[test]
    fn test_estimate_duration_human_minutes() {
        let stats = PlanStats {
            total_bytes: 300_000_000, // 300 MB
            total_files: 100,
            ..Default::default()
        };

        // At 5 MB/s
        let bytes_per_sec = 5 * 1024 * 1024;
        let estimate = stats.estimate_duration_human(bytes_per_sec);
        // 300 MB / 5 MB/s = 57 seconds + 1s overhead = 58s
        assert_eq!(estimate, "58s");
    }

    #[test]
    fn test_estimate_duration_human_minutes_seconds() {
        let stats = PlanStats {
            total_bytes: 150_000_000, // 150 MB
            total_files: 50,
            ..Default::default()
        };

        // At 10 MB/s
        let bytes_per_sec = 10 * 1024 * 1024;
        let estimate = stats.estimate_duration_human(bytes_per_sec);
        // 150 MB / 10 MB/s = 14 seconds (integer division) + 0.5s overhead
        assert_eq!(estimate, "14s");
    }

    #[test]
    fn test_estimate_duration_human_hours() {
        let stats = PlanStats {
            total_bytes: 10_000_000_000, // 10 GB
            total_files: 1000,
            ..Default::default()
        };

        // At 1 MB/s (slow network)
        let bytes_per_sec = 1024 * 1024;
        let estimate = stats.estimate_duration_human(bytes_per_sec);
        // 10 GB / 1 MB/s = 9536 seconds + 10s overhead = 9546s = 2h 39m
        assert_eq!(estimate, "2h 39m");
    }

    #[test]
    fn test_estimate_duration_human_hours_only() {
        let stats = PlanStats {
            total_bytes: 3_600_000_000, // ~3.6 GB
            total_files: 100,
            ..Default::default()
        };

        // At 1 MB/s
        let bytes_per_sec = 1024 * 1024;
        let estimate = stats.estimate_duration_human(bytes_per_sec);
        // 3.6 GB / 1 MB/s = 3433 seconds + 1s overhead = 3434s = 57m 14s
        assert_eq!(estimate, "57m 14s");
    }

    #[test]
    fn test_estimate_duration_human_zero() {
        let stats = PlanStats {
            total_bytes: 0,
            total_files: 0,
            ..Default::default()
        };

        let estimate = stats.estimate_duration_human(100 * 1024 * 1024);
        assert_eq!(estimate, "0s");
    }
}
