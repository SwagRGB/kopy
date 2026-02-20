//! Parallel directory walker based on ignore crate's parallel traversal.

use crate::config::Config;
use crate::scanner::walker::{
    compile_patterns, is_destination_internal_trash, should_include_path, ProgressCallback,
};
use crate::types::{FileEntry, FileTree, KopyError};
use ignore::WalkState;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Default)]
struct CollectedScan {
    files: Vec<FileEntry>,
    total_dirs: usize,
    fatal_error: Option<KopyError>,
}

#[derive(Default)]
struct ProgressState {
    files: u64,
    bytes: u64,
}

/// Scan a directory in parallel and build a `FileTree`.
///
/// This uses `ignore` crate's native parallel traversal to preserve `.gitignore` semantics.
pub fn scan_directory_parallel(
    root_path: &Path,
    config: &Config,
    on_progress: Option<&ProgressCallback>,
) -> Result<FileTree, KopyError> {
    let start_time = Instant::now();

    let exclude_patterns = compile_patterns(&config.exclude_patterns)?;
    let include_patterns = compile_patterns(&config.include_patterns)?;

    let root = root_path.to_path_buf();
    let cfg = config.clone();
    let collected = Arc::new(Mutex::new(CollectedScan::default()));
    let thread_count = config.threads.max(1);
    let progress = Arc::new(Mutex::new(ProgressState::default()));

    let walker = ignore::WalkBuilder::new(root_path)
        .threads(thread_count)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .add_custom_ignore_filename(".kopyignore")
        .build_parallel();

    walker.run(|| {
        let collected = Arc::clone(&collected);
        let exclude_patterns = exclude_patterns.clone();
        let include_patterns = include_patterns.clone();
        let root = root.clone();
        let cfg = cfg.clone();
        let progress = Arc::clone(&progress);

        Box::new(move |result| {
            let scan = match collected.lock() {
                Ok(s) => s,
                Err(_) => return WalkState::Quit,
            };

            if scan.fatal_error.is_some() {
                return WalkState::Quit;
            }
            drop(scan);

            let entry = match result {
                Ok(entry) => entry,
                Err(e) => {
                    eprintln!(
                        "Warning: Error during directory traversal: {}. \
                         Scan will continue with remaining files.",
                        e
                    );
                    return WalkState::Continue;
                }
            };

            let file_type = match entry.file_type() {
                Some(ft) => ft,
                None => return WalkState::Continue,
            };

            let relative_path = match entry.path().strip_prefix(&root) {
                Ok(p) => p.to_path_buf(),
                Err(_) => {
                    eprintln!(
                        "Warning: Failed to calculate relative path for {}. \
                         This may indicate a symlink pointing outside the scan directory. File will be skipped.",
                        entry.path().display()
                    );
                    return WalkState::Continue;
                }
            };

            if !should_include_path(&relative_path, &exclude_patterns, &include_patterns) {
                return WalkState::Continue;
            }

            if is_destination_internal_trash(&root, &cfg, &relative_path) {
                return WalkState::Continue;
            }

            if file_type.is_dir() {
                let mut scan = match collected.lock() {
                    Ok(s) => s,
                    Err(_) => return WalkState::Quit,
                };
                scan.total_dirs += 1;
                return WalkState::Continue;
            }

            if !file_type.is_file() && !file_type.is_symlink() {
                return WalkState::Continue;
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to read metadata for {}: {}. \
                         Try checking file permissions or if the file was deleted during scan.",
                        entry.path().display(),
                        e
                    );
                    return WalkState::Continue;
                }
            };

            let symlink_target = if metadata.is_symlink() {
                match std::fs::read_link(entry.path()) {
                    Ok(target) => Some(target),
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to read symlink target for {}: {}. \
                             Broken symlink will be skipped.",
                            entry.path().display(),
                            e
                        );
                        return WalkState::Continue;
                    }
                }
            } else {
                None
            };

            #[cfg(unix)]
            let permissions = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode()
            };

            #[cfg(not(unix))]
            let permissions = 0o644;

            let mtime = match metadata.modified() {
                Ok(mtime) => mtime,
                Err(e) => {
                    let mut scan = match collected.lock() {
                        Ok(s) => s,
                        Err(_) => return WalkState::Quit,
                    };
                    scan.fatal_error = Some(KopyError::Io(std::io::Error::other(format!(
                        "Failed to get modification time for {}: {}. \
                         This may indicate an unsupported filesystem or corrupted metadata.",
                        entry.path().display(),
                        e
                    ))));
                    return WalkState::Quit;
                }
            };

            let file_entry = match symlink_target {
                Some(target) => FileEntry::new_symlink(
                    relative_path.clone(),
                    metadata.len(),
                    mtime,
                    permissions,
                    target,
                ),
                None => FileEntry::new(relative_path, metadata.len(), mtime, permissions),
            };

            if let Some(callback) = on_progress {
                let mut state = match progress.lock() {
                    Ok(state) => state,
                    Err(_) => return WalkState::Quit,
                };
                state.files += 1;
                state.bytes += file_entry.size;
                callback(state.files, state.bytes);
            }

            let mut scan = match collected.lock() {
                Ok(s) => s,
                Err(_) => return WalkState::Quit,
            };
            scan.files.push(file_entry);
            WalkState::Continue
        })
    });

    let mut tree = FileTree::new(root_path.to_path_buf());
    let mut scan = collected
        .lock()
        .map_err(|_| KopyError::Validation("Parallel scanner state lock poisoned".to_string()))?;

    if let Some(err) = scan.fatal_error.take() {
        return Err(err);
    }

    for _ in 0..scan.total_dirs {
        tree.increment_dirs();
    }

    for entry in scan.files.drain(..) {
        let relative_path = entry.path.clone();
        tree.insert(relative_path, entry);
    }

    tree.set_scan_duration(start_time.elapsed());
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::scan_directory;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    #[test]
    fn test_parallel_scan_empty_directory() {
        let temp = TempDir::new().expect("create temp dir");
        let tree =
            scan_directory_parallel(temp.path(), &Config::default(), None).expect("scan succeeds");

        assert_eq!(tree.total_files, 0);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_parallel_progress_callback() {
        let temp = TempDir::new().expect("create temp dir");
        fs::write(temp.path().join("a.txt"), b"a").expect("write a");
        fs::write(temp.path().join("b.txt"), b"bb").expect("write b");

        let calls = Arc::new(AtomicU64::new(0));
        let last_files = Arc::new(AtomicU64::new(0));

        let callback: ProgressCallback = {
            let calls = Arc::clone(&calls);
            let last_files = Arc::clone(&last_files);
            Box::new(move |files, _bytes| {
                let prev = last_files.swap(files, Ordering::SeqCst);
                assert!(files >= prev);
                calls.fetch_add(1, Ordering::SeqCst);
            })
        };

        let tree = scan_directory_parallel(temp.path(), &Config::default(), Some(&callback))
            .expect("scan succeeds");
        assert_eq!(calls.load(Ordering::SeqCst), tree.total_files as u64);
    }

    #[test]
    fn test_parallel_progress_callback_is_serialized() {
        let temp = TempDir::new().expect("create temp dir");
        for i in 0..128 {
            fs::write(temp.path().join(format!("f{i}.txt")), b"x").expect("write test file");
        }

        let in_callback = Arc::new(AtomicBool::new(false));
        let overlaps = Arc::new(AtomicU64::new(0));
        let calls = Arc::new(AtomicU64::new(0));

        let callback: ProgressCallback = {
            let in_callback = Arc::clone(&in_callback);
            let overlaps = Arc::clone(&overlaps);
            let calls = Arc::clone(&calls);
            Box::new(move |_files, _bytes| {
                if in_callback
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    overlaps.fetch_add(1, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_micros(50));
                calls.fetch_add(1, Ordering::SeqCst);
                in_callback.store(false, Ordering::SeqCst);
            })
        };

        let tree = scan_directory_parallel(temp.path(), &Config::default(), Some(&callback))
            .expect("scan succeeds");
        assert_eq!(calls.load(Ordering::SeqCst), tree.total_files as u64);
        assert_eq!(overlaps.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_parallel_include_overrides_exclude() {
        let temp = TempDir::new().expect("create temp dir");
        fs::write(temp.path().join("keep.log"), b"keep").expect("write keep");
        fs::write(temp.path().join("drop.log"), b"drop").expect("write drop");

        let config = Config {
            source: temp.path().to_path_buf(),
            destination: temp.path().join("dest"),
            exclude_patterns: vec!["*.log".to_string()],
            include_patterns: vec!["keep.log".to_string()],
            ..Config::default()
        };

        let tree = scan_directory_parallel(temp.path(), &config, None).expect("scan succeeds");
        assert!(tree.contains(&PathBuf::from("keep.log")));
        assert!(!tree.contains(&PathBuf::from("drop.log")));
    }

    #[test]
    fn test_parallel_destination_scan_excludes_kopy_trash() {
        let temp = TempDir::new().expect("create temp dir");
        fs::create_dir_all(temp.path().join(".kopy_trash/2026-01-01")).expect("create trash dir");
        fs::write(temp.path().join(".kopy_trash/2026-01-01/deleted.txt"), b"x")
            .expect("write deleted");
        fs::write(temp.path().join("keep.txt"), b"keep").expect("write keep");

        let config = Config {
            source: temp.path().join("src"),
            destination: temp.path().to_path_buf(),
            ..Config::default()
        };

        let tree = scan_directory_parallel(temp.path(), &config, None).expect("scan succeeds");
        assert!(tree.contains(&PathBuf::from("keep.txt")));
        assert!(!tree.contains(&PathBuf::from(".kopy_trash/2026-01-01/deleted.txt")));
    }

    #[test]
    fn test_parallel_parity_with_sequential() {
        let temp = TempDir::new().expect("create temp dir");
        fs::create_dir_all(temp.path().join(".git")).expect("create .git");
        fs::create_dir_all(temp.path().join("sub")).expect("create sub");
        fs::write(temp.path().join(".gitignore"), "ignored.txt\n").expect("write gitignore");
        fs::write(temp.path().join("visible.txt"), b"v").expect("write visible");
        fs::write(temp.path().join("ignored.txt"), b"i").expect("write ignored");
        fs::write(temp.path().join("sub/inner.txt"), b"inner").expect("write inner");

        let config = Config {
            source: temp.path().to_path_buf(),
            destination: temp.path().join("dest"),
            ..Config::default()
        };

        let sequential = scan_directory(temp.path(), &config, None).expect("sequential scan");
        let parallel = scan_directory_parallel(temp.path(), &config, None).expect("parallel scan");

        assert_eq!(parallel.total_files, sequential.total_files);
        assert_eq!(parallel.total_size, sequential.total_size);

        let seq_paths: HashSet<_> = sequential.paths().cloned().collect();
        let par_paths: HashSet<_> = parallel.paths().cloned().collect();
        assert_eq!(par_paths, seq_paths);
    }

    #[test]
    fn test_parallel_progress_starts_before_scan_completion() {
        let temp = TempDir::new().expect("create temp dir");
        for i in 0..2_000 {
            fs::write(temp.path().join(format!("file_{i}.txt")), b"x").expect("write file");
        }

        let first_progress_elapsed = Arc::new(Mutex::new(None::<Duration>));
        let first_progress_elapsed_cb = Arc::clone(&first_progress_elapsed);
        let start = Instant::now();
        let callback: ProgressCallback = Box::new(move |_, _| {
            let mut first = first_progress_elapsed_cb
                .lock()
                .expect("lock first progress");
            if first.is_none() {
                *first = Some(start.elapsed());
            }
        });

        let _ = scan_directory_parallel(temp.path(), &Config::default(), Some(&callback))
            .expect("scan succeeds");
        let total_elapsed = start.elapsed();

        let first = first_progress_elapsed
            .lock()
            .expect("lock first progress")
            .expect("expected at least one progress update");

        // Live progress should begin well before completion.
        assert!(
            first < total_elapsed.mul_f64(0.6),
            "first progress update ({first:?}) should occur before late-stage completion ({total_elapsed:?})"
        );
    }
}
