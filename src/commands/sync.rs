//! Main sync command

use crate::diff::generate_sync_plan;
use crate::executor::{execute_plan, ExecutionEvent};
use crate::scanner::scan_directory;
use crate::types::KopyError;
use crate::ui::ProgressReporter;
use crate::Config;
use indicatif::HumanBytes;
use std::sync::{Arc, Mutex};

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

    let progress_cb = {
        let reporter = Arc::clone(&reporter);
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
            }
            ExecutionEvent::Complete { stats } => {
                if let Ok(progress) = reporter.lock() {
                    progress.finish_transfer(
                        stats.completed_actions,
                        stats.failed_actions,
                        stats.bytes_copied,
                    );
                }
            }
        }
    };

    execute_plan(&plan, &config, Some(&progress_cb))?;
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
}
