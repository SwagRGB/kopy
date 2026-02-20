//! Main sync command

use crate::diff::{compare_files, generate_sync_plan, DiffPlan};
use crate::executor::{execute_plan, ExecutionEvent};
use crate::scanner::{
    resolve_scan_mode, scan_directory, scan_directory_parallel, ResolvedScanMode,
};
use crate::types::{FileEntry, FileTree, KopyError, SyncAction};
use crate::ui::ProgressReporter;
use crate::Config;
use indicatif::HumanBytes;
use std::io::ErrorKind;
use std::sync::{Arc, Mutex};
use std::{collections::BTreeMap, path::PathBuf};

/// Run a full sync cycle for the provided configuration.
///
/// This scans source and destination, builds a diff plan, prints a summary,
/// and executes actions unless `dry_run` is enabled.
///
/// # Example
/// ```no_run
/// use kopy::{commands::sync::run, Config};
/// use std::path::PathBuf;
///
/// let config = Config {
///     source: PathBuf::from("./src_dir"),
///     destination: PathBuf::from("./dst_dir"),
///     ..Config::default()
/// };
///
/// run(config)?;
/// # Ok::<(), kopy::types::KopyError>(())
/// ```
pub fn run(config: Config) -> Result<(), KopyError> {
    if config.source.is_file() {
        return run_single_file_sync(config);
    }

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
    let src_tree = scan_with_mode(&config.source, &config, Some(&src_progress))?;
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
        scan_with_mode(&config.destination, &config, Some(&dest_progress))?
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

    let transfer_total = plan.stats.total_files;
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
                if let Ok(mut progress) = reporter.lock() {
                    progress.reconcile_transfer_completion(transfer_total, stats.bytes_copied);
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

fn run_single_file_sync(config: Config) -> Result<(), KopyError> {
    if config.delete_mode != crate::types::DeleteMode::None {
        eprintln!("Warning: delete flags are ignored when source is a single file.");
    }

    let source_entry = build_source_file_entry(&config.source)?;
    let mut src_tree = FileTree::new(config.source.clone());
    src_tree.insert(PathBuf::new(), source_entry.clone());

    let resolved_destination = resolve_single_file_destination_path(&config)?;

    let mut dest_tree = FileTree::new(resolved_destination.clone());
    if let Some(dest_entry) = build_destination_file_entry(&resolved_destination)? {
        dest_tree.insert(PathBuf::new(), dest_entry);
    }

    let mut single_file_config = config.clone();
    single_file_config.delete_mode = crate::types::DeleteMode::None;
    single_file_config.destination = resolved_destination;

    let mut plan = DiffPlan::new();
    match dest_tree.get(&PathBuf::new()) {
        None => plan.add_action(SyncAction::CopyNew(source_entry)),
        Some(dest_entry) => plan.add_action(compare_files(
            &source_entry,
            dest_entry,
            &single_file_config,
        )),
    }
    plan.sort_by_path();

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

    let reporter = Arc::new(Mutex::new(ProgressReporter::new()));
    if let Ok(mut progress) = reporter.lock() {
        progress.start_transfer(plan.stats.total_files as u64);
    }

    let transfer_total = plan.stats.total_files;
    let delete_total = plan.stats.delete_count;
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
                if let Ok(mut progress) = reporter.lock() {
                    progress.reconcile_transfer_completion(transfer_total, stats.bytes_copied);
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

    execute_plan(&plan, &single_file_config, Some(&progress_cb))?;
    Ok(())
}

fn build_source_file_entry(source_path: &std::path::Path) -> Result<FileEntry, KopyError> {
    let metadata = std::fs::symlink_metadata(source_path).map_err(KopyError::Io)?;
    let mtime = metadata.modified().map_err(KopyError::Io)?;
    #[cfg(unix)]
    let permissions = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode()
    };
    #[cfg(not(unix))]
    let permissions = 0o644;

    if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(source_path).map_err(KopyError::Io)?;
        Ok(FileEntry::new_symlink(
            PathBuf::new(),
            metadata.len(),
            mtime,
            permissions,
            target,
        ))
    } else {
        Ok(FileEntry::new(
            PathBuf::new(),
            metadata.len(),
            mtime,
            permissions,
        ))
    }
}

fn resolve_single_file_destination_path(config: &Config) -> Result<PathBuf, KopyError> {
    if config.destination.is_dir() {
        let file_name = config
            .source
            .file_name()
            .ok_or_else(|| KopyError::Config("Invalid source file name".to_string()))?;
        Ok(config.destination.join(file_name))
    } else {
        Ok(config.destination.clone())
    }
}

fn build_destination_file_entry(
    destination_file: &std::path::Path,
) -> Result<Option<FileEntry>, KopyError> {
    let metadata = match std::fs::symlink_metadata(destination_file) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(KopyError::Io(err)),
    };

    if metadata.file_type().is_dir() {
        return Err(KopyError::Config(format!(
            "Destination resolves to a directory, expected file path: {}",
            destination_file.display(),
        )));
    }

    let mtime = metadata.modified().map_err(KopyError::Io)?;
    #[cfg(unix)]
    let permissions = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode()
    };
    #[cfg(not(unix))]
    let permissions = 0o644;

    let entry = if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(destination_file).map_err(KopyError::Io)?;
        FileEntry::new_symlink(PathBuf::new(), metadata.len(), mtime, permissions, target)
    } else {
        FileEntry::new(PathBuf::new(), metadata.len(), mtime, permissions)
    };
    Ok(Some(entry))
}

fn scan_with_mode(
    root: &std::path::Path,
    config: &Config,
    progress: Option<&crate::scanner::ProgressCallback>,
) -> Result<crate::types::FileTree, KopyError> {
    match resolve_scan_mode(root, config)? {
        ResolvedScanMode::Sequential => scan_directory(root, config, progress),
        ResolvedScanMode::Parallel => scan_directory_parallel(root, config, progress),
    }
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
    suggestion: Option<String>,
}

impl ErrorRecord {
    fn new(path: Option<&std::path::Path>, error: &KopyError) -> Self {
        let (message, suggestion) = humanize_error(error);
        Self {
            kind: error_kind_label(error),
            path: path.map(PathBuf::from),
            message,
            suggestion,
        }
    }
}

fn humanize_error(error: &KopyError) -> (String, Option<String>) {
    match error {
        KopyError::Io(io) => match io.kind() {
            ErrorKind::NotFound => (
                "File or directory was not found".to_string(),
                Some("Verify the path still exists and retry.".to_string()),
            ),
            ErrorKind::PermissionDenied => (
                "Permission denied while accessing file".to_string(),
                Some("Check file permissions or run with a user that has access.".to_string()),
            ),
            ErrorKind::AlreadyExists => (
                "The destination path already exists as a file or directory".to_string(),
                Some("Remove or rename the conflicting path, then retry.".to_string()),
            ),
            ErrorKind::WriteZero | ErrorKind::BrokenPipe | ErrorKind::UnexpectedEof => (
                "File transfer was interrupted before completion".to_string(),
                Some("Retry the sync and check disk/network stability.".to_string()),
            ),
            _ => (
                format!("I/O operation failed: {}", io),
                Some(
                    "Retry the sync. If this keeps happening, check disk health and permissions."
                        .to_string(),
                ),
            ),
        },
        KopyError::PermissionDenied { .. } => (
            "Permission denied while accessing file".to_string(),
            Some("Check file permissions or run with a user that has access.".to_string()),
        ),
        KopyError::DiskFull { .. } => (
            "Not enough disk space to complete operation".to_string(),
            Some("Free disk space on destination and retry.".to_string()),
        ),
        KopyError::ChecksumMismatch { .. } => (
            "File content verification failed".to_string(),
            Some(
                "Re-run with --checksum and inspect source/destination file integrity.".to_string(),
            ),
        ),
        KopyError::TransferInterrupted { .. } => (
            "File transfer was interrupted before completion".to_string(),
            Some("Retry the sync. If this keeps happening, check system stability.".to_string()),
        ),
        KopyError::Config(msg) | KopyError::Validation(msg) => (msg.clone(), None),
        KopyError::SshError(msg) => (
            msg.clone(),
            Some("Check SSH connectivity and credentials.".to_string()),
        ),
        KopyError::DryRun => ("Dry-run mode: no changes were made".to_string(), None),
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
            lines.push(format!("    - {}", record.message));
            if let Some(path) = &record.path {
                lines.push(format!("      Path: {}", path.display()));
            }
            if let Some(suggestion) = &record.suggestion {
                lines.push(format!("      Try: {}", suggestion));
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
                message: "Permission denied while accessing file".to_string(),
                suggestion: Some(
                    "Check file permissions or run with a user that has access.".to_string(),
                ),
            },
            ErrorRecord {
                kind: "Disk full",
                path: Some(PathBuf::from("b.txt")),
                message: "Not enough disk space to complete operation".to_string(),
                suggestion: Some("Free disk space on destination and retry.".to_string()),
            },
            ErrorRecord {
                kind: "Permission denied",
                path: Some(PathBuf::from("c.txt")),
                message: "Permission denied while creating output file".to_string(),
                suggestion: Some(
                    "Check file permissions or run with a user that has access.".to_string(),
                ),
            },
        ];

        let summary = format_error_summary(&records);
        assert!(summary.contains("Error summary:"));
        assert!(summary.contains("Permission denied (2):"));
        assert!(summary.contains("Disk full (1):"));
        assert!(summary.contains("Path: a.txt"));
        assert!(summary.contains("Try: Check file permissions or run with a user that has access."));
    }

    #[test]
    fn test_error_record_io_error_is_plain_english_with_suggestion() {
        let err = KopyError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "File exists",
        ));
        let record = ErrorRecord::new(Some(std::path::Path::new("nested/file.txt")), &err);

        assert_eq!(record.kind, "I/O error");
        assert!(record
            .message
            .contains("destination path already exists as a file"));
        assert!(record
            .path
            .as_ref()
            .is_some_and(|p| p == &PathBuf::from("nested/file.txt")));
        assert!(record
            .suggestion
            .as_deref()
            .is_some_and(|s| s.contains("Remove or rename the conflicting path")));
    }
}
