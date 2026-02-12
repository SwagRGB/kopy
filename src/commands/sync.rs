//! Main sync command

use crate::diff::generate_sync_plan;
use crate::executor::{execute_plan, ExecutionEvent};
use crate::scanner::scan_directory;
use crate::types::KopyError;
use crate::ui::ProgressReporter;
use crate::Config;
use indicatif::HumanBytes;
use std::sync::{Arc, Mutex};
use std::{collections::BTreeMap, path::PathBuf};

/// Run the sync operation
pub fn run(config: Config) -> Result<(), KopyError> {
    let reporter = Arc::new(Mutex::new(ProgressReporter::new()));

    if let Ok(progress) = reporter.lock() {
        progress.start_scan("source");
    }
    let src_progress: crate::scanner::ProgressCallback = {
        let reporter = Arc::clone(&reporter);
        Box::new(move |files: u64, bytes: u64| {
            if let Ok(progress) = reporter.lock() {
                progress.update_scan("source", files, bytes);
            }
        })
    };
    let src_tree = scan_directory(&config.source, &config, Some(&src_progress))?;
    if let Ok(progress) = reporter.lock() {
        progress.finish_scan("source", src_tree.total_files, src_tree.total_size);
        progress.start_scan("destination");
    }

    let dest_tree = if config.destination.exists() {
        let dest_progress: crate::scanner::ProgressCallback = {
            let reporter = Arc::clone(&reporter);
            Box::new(move |files: u64, bytes: u64| {
                if let Ok(progress) = reporter.lock() {
                    progress.update_scan("destination", files, bytes);
                }
            })
        };
        scan_directory(&config.destination, &config, Some(&dest_progress))?
    } else {
        crate::types::FileTree::new(config.destination.clone())
    };
    if let Ok(progress) = reporter.lock() {
        progress.finish_scan("destination", dest_tree.total_files, dest_tree.total_size);
    }

    let plan = generate_sync_plan(&src_tree, &dest_tree, &config);
    print_plan_summary(&plan);

    if config.dry_run {
        print_dry_run_actions(&plan);
        println!("Dry-run mode: no changes were made.");
        return Ok(());
    }

    if !has_executable_actions(&plan) {
        println!("Nothing to sync.");
        return Ok(());
    }

    if let Ok(mut progress) = reporter.lock() {
        progress.start_transfer(plan.stats.total_files as u64);
    }

    let transfer_total = plan.stats.total_files as usize;
    let delete_total = plan.stats.delete_count;
    let error_records: Arc<Mutex<Vec<ErrorRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let progress_cb = {
        let reporter = Arc::clone(&reporter);
        let error_records = Arc::clone(&error_records);
        move |event: &ExecutionEvent| match event {
            ExecutionEvent::ActionStart { action, path, .. } => {
                if let Ok(progress) = reporter.lock() {
                    progress.set_current_file(action, path.as_deref());
                }
            }
            ExecutionEvent::ActionSuccess {
                action,
                bytes_copied,
                ..
            } => {
                // Advance transfer file progress for successful copy/update actions,
                // including zero-byte files.
                if is_transfer_action(action) {
                    if let Ok(mut progress) = reporter.lock() {
                        progress.complete_transfer_file(*bytes_copied);
                    }
                }
            }
            ExecutionEvent::ActionError {
                action,
                path,
                error,
                ..
            } => {
                if let Ok(progress) = reporter.lock() {
                    progress.transfer_error(action, path.as_deref(), &error.to_string());
                }
                if let Ok(mut records) = error_records.lock() {
                    records.push(ErrorRecord::new(path.as_deref(), error));
                }
            }
            ExecutionEvent::Complete { stats } => {
                if let Ok(progress) = reporter.lock() {
                    progress.finish_transfer(
                        stats.completed_actions,
                        stats.failed_actions,
                        stats.bytes_copied,
                        transfer_total,
                        delete_total,
                    );
                }
            }
        }
    };

    let result = execute_plan(&plan, &config, Some(&progress_cb));
    if let Ok(records) = error_records.lock() {
        if !records.is_empty() {
            println!("{}", format_error_summary(&records));
        }
    }

    result?;
    Ok(())
}

fn is_transfer_action(action: &str) -> bool {
    matches!(action, "Copy" | "Update")
}

fn has_executable_actions(plan: &crate::diff::DiffPlan) -> bool {
    plan.actions.iter().any(|action| !action.is_skip())
}

fn print_plan_summary(plan: &crate::diff::DiffPlan) {
    println!("{}", format_plan_preview(plan));
}

fn format_plan_preview(plan: &crate::diff::DiffPlan) -> String {
    format!(
        "Plan:\n  Copy: {}  Update: {}  Delete: {}  Skip: {}\n  Total bytes to transfer: {}",
        plan.stats.copy_count,
        plan.stats.overwrite_count,
        plan.stats.delete_count,
        plan.stats.skip_count,
        HumanBytes(plan.stats.total_bytes)
    )
}

fn print_dry_run_actions(plan: &crate::diff::DiffPlan) {
    println!("{}", format_dry_run_actions(plan));
}

fn format_dry_run_actions(plan: &crate::diff::DiffPlan) -> String {
    if plan.actions.is_empty() {
        return "Dry-run actions:\n  (no planned actions)".to_string();
    }

    let mut lines = Vec::with_capacity(plan.actions.len() + 1);
    lines.push("Dry-run actions:".to_string());
    let mut skipped = 0usize;
    for action in &plan.actions {
        match action {
            crate::types::SyncAction::CopyNew(entry) => {
                lines.push(format!("  COPY      {}", entry.path.display()));
            }
            crate::types::SyncAction::Overwrite(entry) => {
                lines.push(format!("  UPDATE    {}", entry.path.display()));
            }
            crate::types::SyncAction::Delete(path) => {
                lines.push(format!("  DELETE    {}", path.display()));
            }
            crate::types::SyncAction::Skip => {
                skipped += 1;
            }
            crate::types::SyncAction::Move { from, to } => {
                lines.push(format!(
                    "  MOVE      {} -> {}",
                    from.display(),
                    to.display()
                ));
            }
        }
    }

    if skipped > 0 {
        lines.push(format!("  ({skipped} unchanged file(s) omitted)"));
    }

    lines.join("\n")
}

#[derive(Debug)]
struct ErrorRecord {
    kind: &'static str,
    path: Option<PathBuf>,
    message: String,
}

impl ErrorRecord {
    fn new(path: Option<&std::path::Path>, error: &KopyError) -> Self {
        Self {
            kind: error_kind_label(error),
            path: path.map(PathBuf::from),
            message: error.to_string(),
        }
    }
}

fn error_kind_label(error: &KopyError) -> &'static str {
    match error {
        KopyError::Io(_) => "I/O error",
        KopyError::Config(_) => "Configuration error",
        KopyError::Validation(_) => "Validation error",
        KopyError::PermissionDenied { .. } => "Permission denied",
        KopyError::DiskFull { .. } => "Disk full",
        KopyError::ChecksumMismatch { .. } => "Checksum mismatch",
        KopyError::TransferInterrupted { .. } => "Transfer interrupted",
        KopyError::SshError(_) => "SSH error",
        KopyError::DryRun => "Dry run",
    }
}

fn format_error_summary(records: &[ErrorRecord]) -> String {
    let mut groups: BTreeMap<&'static str, Vec<&ErrorRecord>> = BTreeMap::new();
    for record in records {
        groups.entry(record.kind).or_default().push(record);
    }

    let mut lines = Vec::new();
    lines.push("Error summary:".to_string());
    for (kind, items) in groups {
        lines.push(format!("  {} ({}):", kind, items.len()));
        for record in items.iter().take(3) {
            if let Some(path) = &record.path {
                lines.push(format!("    - {} ({})", record.message, path.display()));
            } else {
                lines.push(format!("    - {}", record.message));
            }
        }
        if items.len() > 3 {
            lines.push(format!("    - ... {} more", items.len() - 3));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::DiffPlan;
    use crate::types::{FileEntry, SyncAction};
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn test_is_transfer_action() {
        assert!(is_transfer_action("Copy"));
        assert!(is_transfer_action("Update"));
        assert!(!is_transfer_action("Delete"));
        assert!(!is_transfer_action("Skip"));
        assert!(!is_transfer_action("Move"));
    }

    #[test]
    fn test_has_executable_actions_skip_only_is_false() {
        let mut plan = crate::diff::DiffPlan::new();
        plan.add_action(SyncAction::Skip);
        plan.add_action(SyncAction::Skip);
        assert!(!has_executable_actions(&plan));
    }

    #[test]
    fn test_has_executable_actions_transfer_is_true() {
        let mut plan = crate::diff::DiffPlan::new();
        plan.add_action(SyncAction::Skip);
        plan.add_action(SyncAction::CopyNew(FileEntry::new(
            PathBuf::from("a.txt"),
            0,
            UNIX_EPOCH + Duration::from_secs(1_000),
            0o644,
        )));
        assert!(has_executable_actions(&plan));
    }

    #[test]
    fn test_format_plan_preview_contains_action_counts() {
        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::CopyNew(FileEntry::new(
            PathBuf::from("copy.txt"),
            1024,
            UNIX_EPOCH + Duration::from_secs(1_000),
            0o644,
        )));
        plan.add_action(SyncAction::Overwrite(FileEntry::new(
            PathBuf::from("update.txt"),
            2048,
            UNIX_EPOCH + Duration::from_secs(2_000),
            0o644,
        )));
        plan.add_action(SyncAction::Delete(PathBuf::from("delete.txt")));
        plan.add_action(SyncAction::Skip);

        let preview = format_plan_preview(&plan);
        assert!(preview.contains("Copy: 1"));
        assert!(preview.contains("Update: 1"));
        assert!(preview.contains("Delete: 1"));
        assert!(preview.contains("Skip: 1"));
    }

    #[test]
    fn test_format_plan_preview_uses_human_readable_total_bytes() {
        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::CopyNew(FileEntry::new(
            PathBuf::from("big.bin"),
            5 * 1024 * 1024,
            UNIX_EPOCH + Duration::from_secs(1_000),
            0o644,
        )));

        let preview = format_plan_preview(&plan);
        assert!(
            preview.contains("Total bytes to transfer:") && preview.contains("MiB"),
            "expected human-readable size in preview, got: {preview}"
        );
    }

    #[test]
    fn test_format_dry_run_actions_lists_planned_actions() {
        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::CopyNew(FileEntry::new(
            PathBuf::from("copy.txt"),
            1,
            UNIX_EPOCH + Duration::from_secs(1_000),
            0o644,
        )));
        plan.add_action(SyncAction::Overwrite(FileEntry::new(
            PathBuf::from("update.txt"),
            2,
            UNIX_EPOCH + Duration::from_secs(2_000),
            0o644,
        )));
        plan.add_action(SyncAction::Delete(PathBuf::from("delete.txt")));
        plan.add_action(SyncAction::Skip);

        let preview = format_dry_run_actions(&plan);
        assert!(preview.contains("Dry-run actions:"));
        assert!(preview.contains("COPY      copy.txt"));
        assert!(preview.contains("UPDATE    update.txt"));
        assert!(preview.contains("DELETE    delete.txt"));
        assert!(preview.contains("unchanged file(s) omitted"));
    }

    #[test]
    fn test_format_dry_run_actions_handles_empty_plan() {
        let plan = DiffPlan::new();
        let preview = format_dry_run_actions(&plan);
        assert!(preview.contains("(no planned actions)"));
    }

    #[test]
    fn test_format_error_summary_groups_by_kind() {
        let records = vec![
            ErrorRecord {
                kind: "Permission denied",
                path: Some(PathBuf::from("a.txt")),
                message: "Permission denied: a.txt".to_string(),
            },
            ErrorRecord {
                kind: "Disk full",
                path: Some(PathBuf::from("b.txt")),
                message: "Disk full: 1 bytes available, 2 bytes needed".to_string(),
            },
            ErrorRecord {
                kind: "Permission denied",
                path: Some(PathBuf::from("c.txt")),
                message: "Permission denied: c.txt".to_string(),
            },
        ];

        let summary = format_error_summary(&records);
        assert!(summary.contains("Error summary:"));
        assert!(summary.contains("Permission denied (2):"));
        assert!(summary.contains("Disk full (1):"));
    }
}
