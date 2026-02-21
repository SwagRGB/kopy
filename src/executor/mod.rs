//! Executor module for file operations

pub mod copy;
pub mod pool;
pub mod trash;

use crate::diff::DiffPlan;
use crate::types::{DeleteMode, KopyError, SyncAction};
use crate::Config;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::thread;

/// Execution progress statistics for a sync run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionStats {
    /// Number of actions in the input plan.
    pub total_actions: usize,
    /// Number of successfully processed actions.
    pub completed_actions: usize,
    /// Number of failed actions.
    pub failed_actions: usize,
    /// Aggregate copied bytes (CopyNew + Overwrite).
    pub bytes_copied: u64,
}

/// Events emitted while executing a plan.
#[derive(Debug)]
pub enum ExecutionEvent {
    /// Action execution started.
    ActionStart {
        index: usize,
        total: usize,
        action: &'static str,
        path: Option<PathBuf>,
    },
    /// Action execution succeeded.
    ActionSuccess {
        index: usize,
        total: usize,
        action: &'static str,
        path: Option<PathBuf>,
        bytes_copied: u64,
    },
    /// Action execution failed but executor continued.
    ActionError {
        index: usize,
        total: usize,
        action: &'static str,
        path: Option<PathBuf>,
        error: KopyError,
    },
    /// Plan execution completed (with or without errors).
    Complete { stats: ExecutionStats },
}

/// Optional callback used to receive execution events.
pub type ExecutionCallback = dyn Fn(&ExecutionEvent) + Send + Sync;

pub use copy::copy_file_atomic;
pub use pool::{ParallelExecutor, PoolStats, TransferJob};
pub use trash::move_to_trash;

const LARGE_TRANSFER_THRESHOLD_BYTES: u64 = 16 * 1024 * 1024;

/// Execute a sync plan
///
/// Executes actions sequentially, continues on per-file failures, and returns
/// an aggregated error summary if any action fails.
///
/// # Example
/// ```no_run
/// use kopy::executor::execute_plan;
/// use kopy::diff::DiffPlan;
/// use kopy::Config;
/// use std::path::PathBuf;
///
/// let plan = DiffPlan::new();
/// let config = Config {
///     source: PathBuf::from("./src_dir"),
///     destination: PathBuf::from("./dst_dir"),
///     ..Config::default()
/// };
///
/// let _stats = execute_plan(&plan, &config, None)?;
/// # Ok::<(), kopy::types::KopyError>(())
/// ```
pub fn execute_plan(
    plan: &DiffPlan,
    config: &Config,
    on_event: Option<&ExecutionCallback>,
) -> Result<ExecutionStats, KopyError> {
    let mut stats = ExecutionStats {
        total_actions: plan.actions.len(),
        ..Default::default()
    };
    let mut errors: Vec<(Option<PathBuf>, KopyError)> = Vec::new();

    for (idx, action) in plan.actions.iter().enumerate() {
        let index = idx + 1;
        emit_event(
            on_event,
            ExecutionEvent::ActionStart {
                index,
                total: stats.total_actions,
                action: action.action_name(),
                path: action.path().cloned(),
            },
        );

        let action_result = execute_action(action, config);

        match action_result {
            Ok(bytes) => {
                stats.completed_actions += 1;
                stats.bytes_copied += bytes;

                emit_event(
                    on_event,
                    ExecutionEvent::ActionSuccess {
                        index,
                        total: stats.total_actions,
                        action: action.action_name(),
                        path: action.path().cloned(),
                        bytes_copied: bytes,
                    },
                );
            }
            Err(err) => {
                stats.failed_actions += 1;

                emit_event(
                    on_event,
                    ExecutionEvent::ActionError {
                        index,
                        total: stats.total_actions,
                        action: action.action_name(),
                        path: action.path().cloned(),
                        error: clone_error_for_event(&err),
                    },
                );

                errors.push((action.path().cloned(), err));
            }
        }
    }

    emit_event(
        on_event,
        ExecutionEvent::Complete {
            stats: stats.clone(),
        },
    );

    if errors.is_empty() {
        Ok(stats)
    } else {
        Err(KopyError::Validation(build_error_summary(&errors)))
    }
}

/// Execute a sync plan using parallel workers for small transfer actions.
///
/// Ordering contract:
/// - Small transfer actions (CopyNew/Overwrite <= threshold) run concurrently.
/// - Large transfer actions and non-transfer actions run sequentially.
/// - Sequential actions form ordering barriers: queued small transfers are drained first.
pub fn execute_plan_parallel(
    plan: &DiffPlan,
    config: &Config,
    on_event: Option<&ExecutionCallback>,
) -> Result<ExecutionStats, KopyError> {
    let mut stats = ExecutionStats {
        total_actions: plan.actions.len(),
        ..Default::default()
    };
    let mut errors: Vec<(Option<PathBuf>, KopyError)> = Vec::new();

    let worker_count = config.threads.max(1);
    let total = stats.total_actions;
    let shared_config = config.clone();
    let mut in_flight: Vec<thread::JoinHandle<ParallelTransferResult>> = Vec::new();

    for (idx, action) in plan.actions.iter().enumerate() {
        let index = idx + 1;
        if is_small_parallel_transfer(action) {
            emit_event(
                on_event,
                ExecutionEvent::ActionStart {
                    index,
                    total,
                    action: action.action_name(),
                    path: action.path().cloned(),
                },
            );

            let action_clone = action.clone();
            let config_clone = shared_config.clone();
            in_flight.push(thread::spawn(move || {
                let action_name = action_clone.action_name();
                let path = action_clone.path().cloned();
                let result = execute_action(&action_clone, &config_clone);
                ParallelTransferResult {
                    index,
                    total,
                    action_name,
                    path,
                    result,
                }
            }));

            if in_flight.len() >= worker_count {
                let handle = in_flight.remove(0);
                let result = handle.join().map_err(|_| {
                    KopyError::Validation("parallel worker thread panicked".to_string())
                })?;
                apply_parallel_result(result, on_event, &mut stats, &mut errors);
            }
            continue;
        }

        drain_parallel_handles(&mut in_flight, on_event, &mut stats, &mut errors)?;
        execute_action_with_events(
            action,
            index,
            total,
            &shared_config,
            on_event,
            &mut stats,
            &mut errors,
        );
    }

    drain_parallel_handles(&mut in_flight, on_event, &mut stats, &mut errors)?;

    emit_event(
        on_event,
        ExecutionEvent::Complete {
            stats: stats.clone(),
        },
    );

    if errors.is_empty() {
        Ok(stats)
    } else {
        Err(KopyError::Validation(build_error_summary(&errors)))
    }
}

fn execute_action(action: &SyncAction, config: &Config) -> Result<u64, KopyError> {
    match action {
        SyncAction::CopyNew(entry) | SyncAction::Overwrite(entry) => {
            let (src_path, dest_path) = resolve_transfer_paths(config, &entry.path)?;
            if entry.is_symlink {
                copy_symlink(&src_path, &dest_path, entry)
            } else {
                copy_file_atomic(&src_path, &dest_path, config)
            }
        }
        SyncAction::Delete(path) => execute_delete(path, config).map(|_| 0),
        SyncAction::Skip => Ok(0),
        SyncAction::Move { .. } => Err(KopyError::Validation(
            "Move action is not supported by this executor".to_string(),
        )),
    }
}

#[derive(Debug)]
struct ParallelTransferResult {
    index: usize,
    total: usize,
    action_name: &'static str,
    path: Option<PathBuf>,
    result: Result<u64, KopyError>,
}

fn execute_action_with_events(
    action: &SyncAction,
    index: usize,
    total: usize,
    config: &Config,
    on_event: Option<&ExecutionCallback>,
    stats: &mut ExecutionStats,
    errors: &mut Vec<(Option<PathBuf>, KopyError)>,
) {
    emit_event(
        on_event,
        ExecutionEvent::ActionStart {
            index,
            total,
            action: action.action_name(),
            path: action.path().cloned(),
        },
    );

    match execute_action(action, config) {
        Ok(bytes) => {
            stats.completed_actions += 1;
            stats.bytes_copied += bytes;
            emit_event(
                on_event,
                ExecutionEvent::ActionSuccess {
                    index,
                    total,
                    action: action.action_name(),
                    path: action.path().cloned(),
                    bytes_copied: bytes,
                },
            );
        }
        Err(err) => {
            stats.failed_actions += 1;
            emit_event(
                on_event,
                ExecutionEvent::ActionError {
                    index,
                    total,
                    action: action.action_name(),
                    path: action.path().cloned(),
                    error: clone_error_for_event(&err),
                },
            );
            errors.push((action.path().cloned(), err));
        }
    }
}

fn apply_parallel_result(
    result: ParallelTransferResult,
    on_event: Option<&ExecutionCallback>,
    stats: &mut ExecutionStats,
    errors: &mut Vec<(Option<PathBuf>, KopyError)>,
) {
    match result.result {
        Ok(bytes) => {
            stats.completed_actions += 1;
            stats.bytes_copied += bytes;
            emit_event(
                on_event,
                ExecutionEvent::ActionSuccess {
                    index: result.index,
                    total: result.total,
                    action: result.action_name,
                    path: result.path,
                    bytes_copied: bytes,
                },
            );
        }
        Err(err) => {
            stats.failed_actions += 1;
            emit_event(
                on_event,
                ExecutionEvent::ActionError {
                    index: result.index,
                    total: result.total,
                    action: result.action_name,
                    path: result.path.clone(),
                    error: clone_error_for_event(&err),
                },
            );
            errors.push((result.path, err));
        }
    }
}

fn drain_parallel_handles(
    in_flight: &mut Vec<thread::JoinHandle<ParallelTransferResult>>,
    on_event: Option<&ExecutionCallback>,
    stats: &mut ExecutionStats,
    errors: &mut Vec<(Option<PathBuf>, KopyError)>,
) -> Result<(), KopyError> {
    while let Some(handle) = in_flight.pop() {
        let result = handle
            .join()
            .map_err(|_| KopyError::Validation("parallel worker thread panicked".to_string()))?;
        apply_parallel_result(result, on_event, stats, errors);
    }
    Ok(())
}

fn is_small_parallel_transfer(action: &SyncAction) -> bool {
    if !action.requires_transfer() {
        return false;
    }
    let size = action.file_entry().map(|entry| entry.size).unwrap_or(0);
    size <= LARGE_TRANSFER_THRESHOLD_BYTES
}

fn resolve_transfer_paths(
    config: &Config,
    relative_path: &std::path::Path,
) -> Result<(PathBuf, PathBuf), KopyError> {
    if config.source.is_file() {
        let src_path = config.source.clone();
        let dest_path = if config.destination.is_dir() {
            let file_name = config.source.file_name().ok_or_else(|| {
                KopyError::Config(format!(
                    "Unable to determine source file name: {}",
                    config.source.display()
                ))
            })?;
            config.destination.join(file_name)
        } else {
            config.destination.clone()
        };
        Ok((src_path, dest_path))
    } else {
        Ok((
            config.source.join(relative_path),
            config.destination.join(relative_path),
        ))
    }
}

/// Copy a symlink entry without dereferencing its target.
///
/// If a destination path already exists, it is removed first (file/dir/symlink).
fn copy_symlink(
    src_path: &std::path::Path,
    dest_path: &std::path::Path,
    entry: &crate::types::FileEntry,
) -> Result<u64, KopyError> {
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent).map_err(KopyError::Io)?;
    }

    if fs::symlink_metadata(dest_path).is_ok() {
        remove_path_any(dest_path).map_err(KopyError::Io)?;
    }

    let target = match &entry.symlink_target {
        Some(t) => t.clone(),
        None => std::fs::read_link(src_path).map_err(KopyError::Io)?,
    };

    create_symlink(&target, dest_path)?;
    Ok(0)
}

/// Remove any filesystem entry at `path`.
///
/// Directories are removed recursively; files and symlinks are removed as files.
fn remove_path_any(path: &std::path::Path) -> Result<(), Error> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(unix)]
fn create_symlink(target: &std::path::Path, link_path: &std::path::Path) -> Result<(), KopyError> {
    std::os::unix::fs::symlink(target, link_path).map_err(KopyError::Io)
}

#[cfg(not(unix))]
fn create_symlink(target: &std::path::Path, link_path: &std::path::Path) -> Result<(), KopyError> {
    let error = std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!(
            "Symlink copy is unsupported on this platform: {} -> {}",
            target.display(),
            link_path.display()
        ),
    );
    Err(KopyError::Io(error))
}

/// Execute delete behavior according to configured delete mode.
///
/// - `None`: no-op (non-destructive)
/// - `Trash`: move entry to `.kopy_trash`
/// - `Permanent`: remove file and treat `NotFound` as success
fn execute_delete(path: &PathBuf, config: &Config) -> Result<(), KopyError> {
    let dest_path = config.destination.join(path);

    match config.delete_mode {
        DeleteMode::None => Ok(()),
        DeleteMode::Trash => {
            if fs::symlink_metadata(&dest_path).is_err() {
                Ok(())
            } else {
                move_to_trash(&dest_path, &config.destination, path, config)
            }
        }
        DeleteMode::Permanent => match fs::symlink_metadata(&dest_path) {
            Ok(_) => remove_with_mapped_delete_error(&dest_path, true),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(map_delete_error(&dest_path, e)),
        },
    }
}

fn remove_with_mapped_delete_error(
    path: &std::path::Path,
    not_found_is_ok: bool,
) -> Result<(), KopyError> {
    match remove_path_any(path) {
        Ok(()) => Ok(()),
        Err(e) if not_found_is_ok && e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(map_delete_error(path, e)),
    }
}

fn map_delete_error(path: &std::path::Path, error: Error) -> KopyError {
    if matches!(error.kind(), ErrorKind::PermissionDenied) {
        KopyError::PermissionDenied {
            path: path.to_path_buf(),
        }
    } else if matches!(error.kind(), ErrorKind::StorageFull)
        || matches!(error.raw_os_error(), Some(28 | 122))
    {
        KopyError::DiskFull {
            available: 0,
            needed: 1,
        }
    } else {
        KopyError::Io(error)
    }
}

fn emit_event(on_event: Option<&ExecutionCallback>, event: ExecutionEvent) {
    if let Some(callback) = on_event {
        callback(&event);
    }
}

fn clone_error_for_event(error: &KopyError) -> KopyError {
    match error {
        KopyError::Io(e) => KopyError::Io(Error::new(e.kind(), e.to_string())),
        KopyError::Config(msg) => KopyError::Config(msg.clone()),
        KopyError::Validation(msg) => KopyError::Validation(msg.clone()),
        KopyError::PermissionDenied { path } => KopyError::PermissionDenied { path: path.clone() },
        KopyError::DiskFull { available, needed } => KopyError::DiskFull {
            available: *available,
            needed: *needed,
        },
        KopyError::ChecksumMismatch { path } => KopyError::ChecksumMismatch { path: path.clone() },
        KopyError::TransferInterrupted { path, offset } => KopyError::TransferInterrupted {
            path: path.clone(),
            offset: *offset,
        },
        KopyError::SshError(msg) => KopyError::SshError(msg.clone()),
        KopyError::DryRun => KopyError::DryRun,
    }
}

fn build_error_summary(errors: &[(Option<PathBuf>, KopyError)]) -> String {
    let preview = errors
        .iter()
        .take(3)
        .map(|(path, err)| {
            let path_display = path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            format!("{}: {}", path_display, err)
        })
        .collect::<Vec<_>>()
        .join("; ");

    format!(
        "Sync completed with {} error(s). Example failures: {}",
        errors.len(),
        preview
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScanMode;
    use crate::types::FileEntry;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, UNIX_EPOCH};
    use tempfile::TempDir;

    fn config_for(source: &TempDir, destination: &TempDir, delete_mode: DeleteMode) -> Config {
        Config {
            source: source.path().to_path_buf(),
            destination: destination.path().to_path_buf(),
            dry_run: false,
            checksum_mode: false,
            delete_mode,
            exclude_patterns: vec![],
            include_patterns: vec![],
            threads: 1,
            scan_mode: ScanMode::Auto,
            bandwidth_limit: None,
            backup_dir: None,
            watch: false,
            watch_settle: 2,
        }
    }

    fn entry(path: &str, size: u64) -> FileEntry {
        FileEntry::new(
            PathBuf::from(path),
            size,
            UNIX_EPOCH + Duration::from_secs(1_000),
            0o644,
        )
    }

    #[test]
    fn test_execute_plan_copy_overwrite_skip() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let config = config_for(&src, &dst, DeleteMode::None);

        fs::write(src.path().join("new.txt"), b"new-content").expect("write src new");
        fs::write(src.path().join("keep.txt"), b"updated").expect("write src keep");
        fs::write(dst.path().join("keep.txt"), b"old").expect("write dst keep old");

        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::CopyNew(entry("new.txt", 11)));
        plan.add_action(SyncAction::Overwrite(entry("keep.txt", 7)));
        plan.add_action(SyncAction::Skip);

        let stats = execute_plan(&plan, &config, None).expect("execute plan");

        assert_eq!(stats.total_actions, 3);
        assert_eq!(stats.completed_actions, 3);
        assert_eq!(stats.failed_actions, 0);
        assert_eq!(
            fs::read(dst.path().join("new.txt")).expect("read dst new"),
            b"new-content"
        );
        assert_eq!(
            fs::read(dst.path().join("keep.txt")).expect("read dst keep"),
            b"updated"
        );
    }

    #[test]
    fn test_execute_plan_delete_trash() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let config = config_for(&src, &dst, DeleteMode::Trash);

        fs::write(dst.path().join("old.txt"), b"to-delete").expect("write dst old");

        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::Delete(PathBuf::from("old.txt")));

        let stats = execute_plan(&plan, &config, None).expect("execute plan");
        assert_eq!(stats.failed_actions, 0);
        assert!(!dst.path().join("old.txt").exists());
        assert!(dst.path().join(".kopy_trash").exists());
    }

    #[test]
    fn test_execute_plan_delete_none_is_noop() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let config = config_for(&src, &dst, DeleteMode::None);

        fs::write(dst.path().join("keep.txt"), b"keep").expect("write dst keep");

        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::Delete(PathBuf::from("keep.txt")));

        let stats = execute_plan(&plan, &config, None).expect("execute plan");
        assert_eq!(stats.failed_actions, 0);
        assert!(dst.path().join("keep.txt").exists());
    }

    #[test]
    fn test_execute_plan_delete_permanent() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let config = config_for(&src, &dst, DeleteMode::Permanent);

        fs::write(dst.path().join("old.txt"), b"to-delete").expect("write dst old");

        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::Delete(PathBuf::from("old.txt")));

        let stats = execute_plan(&plan, &config, None).expect("execute plan");
        assert_eq!(stats.failed_actions, 0);
        assert!(!dst.path().join("old.txt").exists());
    }

    #[test]
    fn test_execute_plan_delete_permanent_missing_file_is_ok() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let config = config_for(&src, &dst, DeleteMode::Permanent);

        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::Delete(PathBuf::from("missing.txt")));

        let stats = execute_plan(&plan, &config, None).expect("execute plan");
        assert_eq!(stats.failed_actions, 0);
        assert_eq!(stats.completed_actions, 1);
    }

    #[test]
    #[cfg(unix)]
    fn test_execute_plan_delete_trash_broken_symlink() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let config = config_for(&src, &dst, DeleteMode::Trash);

        std::os::unix::fs::symlink("missing-target.txt", dst.path().join("broken-link"))
            .expect("create broken symlink");

        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::Delete(PathBuf::from("broken-link")));

        let stats = execute_plan(&plan, &config, None).expect("execute plan");
        assert_eq!(stats.failed_actions, 0);
        assert!(fs::symlink_metadata(dst.path().join("broken-link")).is_err());
        assert!(dst.path().join(".kopy_trash").exists());
    }

    #[test]
    #[cfg(unix)]
    fn test_execute_plan_copy_new_preserves_symlink() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let config = config_for(&src, &dst, DeleteMode::None);

        fs::write(src.path().join("target.txt"), b"payload").expect("write target");
        std::os::unix::fs::symlink("target.txt", src.path().join("link.txt"))
            .expect("create symlink");

        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::CopyNew(FileEntry::new_symlink(
            PathBuf::from("link.txt"),
            0,
            UNIX_EPOCH + Duration::from_secs(2_000),
            0o777,
            PathBuf::from("target.txt"),
        )));

        let stats = execute_plan(&plan, &config, None).expect("execute plan");
        assert_eq!(stats.failed_actions, 0);

        let link_path = dst.path().join("link.txt");
        let metadata = fs::symlink_metadata(&link_path).expect("symlink metadata");
        assert!(
            metadata.file_type().is_symlink(),
            "destination should be symlink"
        );
        let target = fs::read_link(&link_path).expect("read link");
        assert_eq!(target, PathBuf::from("target.txt"));
    }

    #[test]
    fn test_execute_plan_continue_on_error() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let config = config_for(&src, &dst, DeleteMode::None);

        fs::write(src.path().join("good.txt"), b"good").expect("write src good");

        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::CopyNew(entry("missing.txt", 10)));
        plan.add_action(SyncAction::CopyNew(entry("good.txt", 4)));

        let result = execute_plan(&plan, &config, None);
        assert!(result.is_err());
        assert!(dst.path().join("good.txt").exists());
    }

    #[test]
    fn test_execute_plan_emits_events() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let config = config_for(&src, &dst, DeleteMode::None);

        fs::write(src.path().join("new.txt"), b"new-content").expect("write src new");
        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::CopyNew(entry("new.txt", 11)));

        let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let events_ref = Arc::clone(&events);
        let callback = move |event: &ExecutionEvent| {
            let label = match event {
                ExecutionEvent::ActionStart { .. } => "start",
                ExecutionEvent::ActionSuccess { .. } => "success",
                ExecutionEvent::ActionError { .. } => "error",
                ExecutionEvent::Complete { .. } => "complete",
            };
            events_ref
                .lock()
                .expect("lock events")
                .push(label.to_string());
        };

        let stats = execute_plan(&plan, &config, Some(&callback)).expect("execute plan");
        assert_eq!(stats.failed_actions, 0);

        let snapshot = events.lock().expect("lock events snapshot").clone();
        assert_eq!(snapshot, vec!["start", "success", "complete"]);
    }

    #[test]
    fn test_execute_plan_parallel_mixed_transfer_and_delete() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let mut config = config_for(&src, &dst, DeleteMode::Trash);
        config.threads = 4;

        let small_payload = vec![b'a'; 1024];
        fs::write(src.path().join("small.txt"), &small_payload).expect("write small source");

        let large_size = LARGE_TRANSFER_THRESHOLD_BYTES + 1;
        let large_path = src.path().join("large.bin");
        let large_file = fs::File::create(&large_path).expect("create large source");
        large_file
            .set_len(large_size)
            .expect("set large source len");

        fs::write(dst.path().join("old.txt"), b"trash-me").expect("write old destination");

        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::CopyNew(entry(
            "small.txt",
            small_payload.len() as u64,
        )));
        plan.add_action(SyncAction::CopyNew(entry("large.bin", large_size)));
        plan.add_action(SyncAction::Delete(PathBuf::from("old.txt")));

        let stats = execute_plan_parallel(&plan, &config, None).expect("execute parallel plan");
        assert_eq!(stats.total_actions, 3);
        assert_eq!(stats.completed_actions, 3);
        assert_eq!(stats.failed_actions, 0);
        assert_eq!(
            fs::metadata(dst.path().join("small.txt"))
                .expect("metadata small destination")
                .len(),
            small_payload.len() as u64
        );
        assert_eq!(
            fs::metadata(dst.path().join("large.bin"))
                .expect("metadata large destination")
                .len(),
            large_size
        );
        assert!(!dst.path().join("old.txt").exists());
    }

    #[test]
    fn test_execute_plan_parallel_continues_on_error() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let mut config = config_for(&src, &dst, DeleteMode::None);
        config.threads = 4;

        fs::write(src.path().join("good.txt"), b"good").expect("write good source");

        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::CopyNew(entry("missing.txt", 10)));
        plan.add_action(SyncAction::CopyNew(entry("good.txt", 4)));

        let result = execute_plan_parallel(&plan, &config, None);
        assert!(result.is_err());
        assert!(dst.path().join("good.txt").exists());
    }

    #[test]
    fn test_execute_plan_parallel_emits_complete_event() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let mut config = config_for(&src, &dst, DeleteMode::None);
        config.threads = 2;

        fs::write(src.path().join("one.txt"), b"one").expect("write source file");
        let mut plan = DiffPlan::new();
        plan.add_action(SyncAction::CopyNew(entry("one.txt", 3)));

        let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let events_ref = Arc::clone(&events);
        let callback = move |event: &ExecutionEvent| {
            let label = match event {
                ExecutionEvent::ActionStart { .. } => "start",
                ExecutionEvent::ActionSuccess { .. } => "success",
                ExecutionEvent::ActionError { .. } => "error",
                ExecutionEvent::Complete { .. } => "complete",
            };
            events_ref
                .lock()
                .expect("lock events")
                .push(label.to_string());
        };

        let stats =
            execute_plan_parallel(&plan, &config, Some(&callback)).expect("execute parallel plan");
        assert_eq!(stats.failed_actions, 0);

        let snapshot = events.lock().expect("lock events snapshot").clone();
        assert_eq!(snapshot, vec!["start", "success", "complete"]);
    }

    #[test]
    fn test_execute_plan_parallel_many_small_transfers_complete() {
        let src = tempfile::tempdir().expect("create src tempdir");
        let dst = tempfile::tempdir().expect("create dst tempdir");
        let mut config = config_for(&src, &dst, DeleteMode::None);
        config.threads = 2;

        let mut plan = DiffPlan::new();
        for i in 0..200 {
            let name = format!("f_{i}.txt");
            fs::write(src.path().join(&name), b"x").expect("write source");
            plan.add_action(SyncAction::CopyNew(entry(&name, 1)));
        }

        let stats = execute_plan_parallel(&plan, &config, None).expect("execute parallel plan");
        assert_eq!(stats.total_actions, 200);
        assert_eq!(stats.completed_actions, 200);
        assert_eq!(stats.failed_actions, 0);
    }
}
