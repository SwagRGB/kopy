//! Main sync command

use crate::diff::generate_sync_plan;
use crate::executor::{execute_plan, ExecutionEvent};
use crate::scanner::scan_directory;
use crate::types::KopyError;
use crate::ui::ProgressReporter;
use crate::Config;
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
    println!("Plan:");
    println!(
        "  Copy: {}  Update: {}  Delete: {}  Skip: {}",
        plan.stats.copy_count,
        plan.stats.overwrite_count,
        plan.stats.delete_count,
        plan.stats.skip_count
    );
    println!("  Total bytes to transfer: {}", plan.stats.total_bytes);

    for action in &plan.actions {
        match action {
            crate::types::SyncAction::CopyNew(entry) => {
                println!("  COPY      {}", entry.path.display());
            }
            crate::types::SyncAction::Overwrite(entry) => {
                println!("  UPDATE    {}", entry.path.display());
            }
            crate::types::SyncAction::Delete(path) => {
                println!("  DELETE    {}", path.display());
            }
            crate::types::SyncAction::Skip => {
                println!("  SKIP      <unchanged>");
            }
            crate::types::SyncAction::Move { from, to } => {
                println!("  MOVE      {} -> {}", from.display(), to.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
