//! Directory walker

use crate::config::Config;
use crate::types::{FileEntry, FileTree, KopyError};
use std::path::Path;
use std::time::Instant;

/// Callback for reporting scan progress
///
/// Arguments:
/// - `files_scanned`: Total number of files scanned so far
/// - `bytes_scanned`: Total bytes scanned so far
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Scan a directory and build a FileTree
///
/// Walks the directory tree recursively and builds a complete FileTree representation.
/// Uses the `ignore` crate for traversal with filtering support.
///
/// # Arguments
/// * `root_path` - The root directory to scan
/// * `config` - Configuration containing exclude patterns and other settings
/// * `on_progress` - Optional callback for progress updates (files_scanned, bytes_scanned)
///
/// # Returns
/// * `Ok(FileTree)` - Successfully scanned tree with all files and metadata
/// * `Err(KopyError)` - IO error or other failure during scanning
///
/// # Errors
/// * Permission denied errors are logged but don't stop the scan
/// * Broken symlinks are skipped with a warning
/// * Invalid exclude patterns return KopyError::Config
/// * Other IO errors are propagated as KopyError
///
/// Filter precedence:
/// 1. walker-level ignore files (`.gitignore`, `.ignore`, `.git/info/exclude`, `.kopyignore`)
/// 2. CLI pattern check where `--include` overrides `--exclude`
pub fn scan_directory(
    root_path: &Path,
    config: &Config,
    on_progress: Option<&ProgressCallback>,
) -> Result<FileTree, KopyError> {
    let start_time = Instant::now();
    let mut tree = FileTree::new(root_path.to_path_buf());

    let mut scanned_count: u64 = 0;
    let mut scanned_bytes: u64 = 0;

    let exclude_patterns = compile_patterns(&config.exclude_patterns)?;
    let include_patterns = compile_patterns(&config.include_patterns)?;

    let walker = ignore::WalkBuilder::new(root_path)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .add_custom_ignore_filename(".kopyignore")
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                let file_type = match entry.file_type() {
                    Some(ft) => ft,
                    None => continue,
                };

                let relative_path = match entry.path().strip_prefix(root_path) {
                    Ok(p) => p.to_path_buf(),
                    Err(_) => {
                        eprintln!(
                            "Warning: Failed to calculate relative path for {}. \
                             This may indicate a symlink pointing outside the scan directory. File will be skipped.",
                            entry.path().display()
                        );
                        continue;
                    }
                };

                // Apply CLI exclude/include filtering:
                // include patterns explicitly override exclude patterns.
                if !should_include_path(&relative_path, &exclude_patterns, &include_patterns) {
                    continue;
                }

                if is_destination_internal_trash(root_path, config, &relative_path) {
                    continue;
                }

                if file_type.is_dir() {
                    tree.increment_dirs();
                    continue;
                }

                if !file_type.is_file() && !file_type.is_symlink() {
                    continue;
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
                        continue;
                    }
                };

                let (_is_symlink, symlink_target) = if metadata.is_symlink() {
                    match std::fs::read_link(entry.path()) {
                        Ok(target) => (true, Some(target)),
                        Err(e) => {
                            eprintln!(
                                "Warning: Failed to read symlink target for {}: {}. \
                                 Broken symlink will be skipped.",
                                entry.path().display(),
                                e
                            );
                            continue;
                        }
                    }
                } else {
                    (false, None)
                };

                #[cfg(unix)]
                let permissions = {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode()
                };

                #[cfg(not(unix))]
                let permissions = 0o644;

                let mtime = metadata.modified().map_err(|e| {
                    KopyError::Io(std::io::Error::other(format!(
                        "Failed to get modification time for {}: {}. \
                         This may indicate an unsupported filesystem or corrupted metadata.",
                        entry.path().display(),
                        e
                    )))
                })?;

                let file_entry = if let Some(target) = symlink_target {
                    FileEntry::new_symlink(
                        relative_path.clone(),
                        metadata.len(),
                        mtime,
                        permissions,
                        target,
                    )
                } else {
                    FileEntry::new(relative_path.clone(), metadata.len(), mtime, permissions)
                };

                tree.insert(relative_path, file_entry);

                scanned_count += 1;
                scanned_bytes += metadata.len();

                if let Some(callback) = on_progress {
                    callback(scanned_count, scanned_bytes);
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: Error during directory traversal: {}. \
                     Scan will continue with remaining files.",
                    e
                );
                continue;
            }
        }
    }

    let duration = start_time.elapsed();
    tree.set_scan_duration(duration);

    Ok(tree)
}

fn compile_patterns(patterns: &[String]) -> Result<Vec<glob::Pattern>, KopyError> {
    patterns
        .iter()
        .map(|pattern| {
            glob::Pattern::new(pattern)
                .map_err(|e| KopyError::Config(format!("Invalid pattern '{}': {}", pattern, e)))
        })
        .collect()
}

fn should_include_path(
    relative_path: &Path,
    exclude_patterns: &[glob::Pattern],
    include_patterns: &[glob::Pattern],
) -> bool {
    // CLI include patterns override CLI excludes for matched paths.
    let excluded = exclude_patterns
        .iter()
        .any(|pattern| pattern.matches_path(relative_path));
    if !excluded {
        return true;
    }

    include_patterns
        .iter()
        .any(|pattern| pattern.matches_path(relative_path))
}

fn is_destination_internal_trash(root_path: &Path, config: &Config, relative_path: &Path) -> bool {
    root_path == config.destination && relative_path.starts_with(".kopy_trash")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root_path = temp_dir.path();

        let result = scan_directory(root_path, &Config::default(), None);
        assert!(result.is_ok(), "scan_directory should succeed on empty dir");

        let tree = result.unwrap();
        assert!(tree.is_empty(), "Tree should be empty");
        assert_eq!(tree.total_files, 0, "Should have 0 files");
        assert_eq!(tree.total_size, 0, "Should have 0 total size");
        assert_eq!(tree.root_path, root_path);
    }

    #[test]
    fn test_scan_single_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root_path = temp_dir.path();

        // Create a single file
        let file_path = root_path.join("test.txt");
        let mut file = fs::File::create(&file_path).expect("Failed to create file");
        file.write_all(b"Hello, World!").expect("Failed to write");
        drop(file);

        let result = scan_directory(root_path, &Config::default(), None);
        assert!(result.is_ok(), "scan_directory should succeed");

        let tree = result.unwrap();
        assert_eq!(tree.total_files, 1, "Should have 1 file");
        assert_eq!(tree.total_size, 13, "Should have 13 bytes");
        assert!(!tree.is_empty(), "Tree should not be empty");

        // Check the file entry exists
        let relative_path = std::path::PathBuf::from("test.txt");
        assert!(tree.contains(&relative_path), "Should contain test.txt");

        let entry = tree.get(&relative_path).expect("Entry should exist");
        assert_eq!(entry.size, 13);
        assert_eq!(entry.path, relative_path);
        assert!(!entry.is_symlink);
    }

    #[test]
    fn test_scan_nested_directories() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root_path = temp_dir.path();

        // Create nested structure: root/a/b/file.txt, root/c/file2.txt
        fs::create_dir_all(root_path.join("a/b")).expect("Failed to create dirs");
        fs::create_dir(root_path.join("c")).expect("Failed to create dir");

        let file1_path = root_path.join("a/b/file.txt");
        let mut file1 = fs::File::create(&file1_path).expect("Failed to create file1");
        file1.write_all(b"File 1").expect("Failed to write");
        drop(file1);

        let file2_path = root_path.join("c/file2.txt");
        let mut file2 = fs::File::create(&file2_path).expect("Failed to create file2");
        file2.write_all(b"File 2 content").expect("Failed to write");
        drop(file2);

        let result = scan_directory(root_path, &Config::default(), None);
        assert!(result.is_ok(), "scan_directory should succeed");

        let tree = result.unwrap();
        assert_eq!(tree.total_files, 2, "Should have 2 files");
        assert_eq!(tree.total_size, 6 + 14, "Should have 20 bytes total");
        assert!(
            tree.total_dirs >= 3,
            "Should have at least 3 directories (a, b, c)"
        );

        // Check relative paths are correct
        let path1 = std::path::PathBuf::from("a/b/file.txt");
        let path2 = std::path::PathBuf::from("c/file2.txt");
        assert!(tree.contains(&path1), "Should contain a/b/file.txt");
        assert!(tree.contains(&path2), "Should contain c/file2.txt");
    }

    #[test]
    #[cfg(unix)] // Symlinks work differently on Windows
    fn test_scan_with_symlink() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root_path = temp_dir.path();

        // Create a file
        let target_path = root_path.join("target.txt");
        let mut file = fs::File::create(&target_path).expect("Failed to create file");
        file.write_all(b"Target content").expect("Failed to write");
        drop(file);

        // Create a symlink to it
        let link_path = root_path.join("link.txt");
        std::os::unix::fs::symlink(&target_path, &link_path).expect("Failed to create symlink");

        let result = scan_directory(root_path, &Config::default(), None);
        assert!(result.is_ok(), "scan_directory should succeed");

        let tree = result.unwrap();

        // Should have both the target and the symlink
        let link_relative = std::path::PathBuf::from("link.txt");
        assert!(tree.contains(&link_relative), "Should contain the symlink");

        let link_entry = tree
            .get(&link_relative)
            .expect("Symlink entry should exist");
        assert!(link_entry.is_symlink, "Entry should be marked as symlink");
        assert!(
            link_entry.symlink_target.is_some(),
            "Symlink should have target"
        );
    }

    #[test]
    #[cfg(unix)] // Symlinks work differently on Windows
    fn test_scan_broken_symlink() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root_path = temp_dir.path();

        // Create a symlink to a non-existent target
        let link_path = root_path.join("broken_link.txt");
        let fake_target = root_path.join("nonexistent.txt");
        std::os::unix::fs::symlink(&fake_target, &link_path).expect("Failed to create symlink");

        let result = scan_directory(root_path, &Config::default(), None);
        // Should not panic, should complete successfully
        assert!(
            result.is_ok(),
            "scan_directory should handle broken symlinks gracefully"
        );

        // Broken symlink might be skipped or included depending on implementation
        // The key is that it doesn't crash
    }

    #[test]
    fn test_scan_statistics() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root_path = temp_dir.path();

        // Create multiple files with known sizes
        let files = vec![("file1.txt", 100), ("file2.txt", 200), ("file3.txt", 300)];

        let mut expected_size = 0;
        for (name, size) in &files {
            let file_path = root_path.join(name);
            let mut file = fs::File::create(&file_path).expect("Failed to create file");
            let content = vec![b'x'; *size];
            file.write_all(&content).expect("Failed to write");
            expected_size += size;
            drop(file);
        }

        let result = scan_directory(root_path, &Config::default(), None);
        assert!(result.is_ok(), "scan_directory should succeed");

        let tree = result.unwrap();
        assert_eq!(tree.total_files, 3, "Should have 3 files");
        assert_eq!(
            tree.total_size, expected_size as u64,
            "Total size should match"
        );
    }

    #[test]
    fn test_scan_duration_recorded() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root_path = temp_dir.path();

        // Create a file
        let file_path = root_path.join("test.txt");
        fs::File::create(&file_path).expect("Failed to create file");

        let result = scan_directory(root_path, &Config::default(), None);
        assert!(result.is_ok(), "scan_directory should succeed");

        let tree = result.unwrap();
        assert!(
            tree.scan_duration > Duration::from_secs(0),
            "Scan duration should be recorded"
        );
    }

    #[test]
    fn test_respects_gitignore() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir.path();

        // Create .git directory (required for ignore crate to respect .gitignore)
        fs::create_dir(root.join(".git")).expect("Failed to create .git dir");

        // Create .gitignore
        fs::write(root.join(".gitignore"), "*.log\ntemp/\n").expect("Failed to create .gitignore");

        // Create files
        fs::write(root.join("keep.txt"), "keep").expect("Failed to create keep.txt");
        fs::write(root.join("ignore.log"), "ignore").expect("Failed to create ignore.log");
        fs::create_dir(root.join("temp")).expect("Failed to create temp dir");
        fs::write(root.join("temp/file.txt"), "ignore").expect("Failed to create temp/file.txt");

        let config = Config::default();
        let tree = scan_directory(root, &config, None).expect("scan_directory should succeed");

        // Should only contain keep.txt (and .gitignore itself)
        assert!(
            tree.contains(&PathBuf::from("keep.txt")),
            "Should contain keep.txt"
        );
        assert!(
            !tree.contains(&PathBuf::from("ignore.log")),
            "Should NOT contain ignore.log"
        );
        assert!(
            !tree.contains(&PathBuf::from("temp/file.txt")),
            "Should NOT contain temp/file.txt"
        );
    }

    #[test]
    fn test_respects_kopyignore() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir.path();

        // Create .kopyignore
        fs::write(root.join(".kopyignore"), "*.tmp\ncache/\n")
            .expect("Failed to create .kopyignore");

        // Create files
        fs::write(root.join("keep.txt"), "keep").expect("Failed to create keep.txt");
        fs::write(root.join("ignore.tmp"), "ignore").expect("Failed to create ignore.tmp");
        fs::create_dir(root.join("cache")).expect("Failed to create cache dir");
        fs::write(root.join("cache/data.txt"), "ignore").expect("Failed to create cache/data.txt");

        let config = Config::default();
        let tree = scan_directory(root, &config, None).expect("scan_directory should succeed");

        // Should only contain keep.txt (and .kopyignore itself)
        assert!(
            tree.contains(&PathBuf::from("keep.txt")),
            "Should contain keep.txt"
        );
        assert!(
            !tree.contains(&PathBuf::from("ignore.tmp")),
            "Should NOT contain ignore.tmp"
        );
        assert!(
            !tree.contains(&PathBuf::from("cache/data.txt")),
            "Should NOT contain cache/data.txt"
        );
    }

    #[test]
    fn test_respects_cli_exclude() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir.path();

        // Create files
        fs::write(root.join("keep.txt"), "keep").expect("Failed to create keep.txt");
        fs::write(root.join("ignore.log"), "ignore").expect("Failed to create ignore.log");
        fs::write(root.join("debug.log"), "ignore").expect("Failed to create debug.log");

        // Config with exclude pattern
        let config = Config {
            source: root.to_path_buf(),
            destination: PathBuf::from("/tmp/dest"),
            exclude_patterns: vec!["*.log".to_string()],
            ..Default::default()
        };

        let tree = scan_directory(root, &config, None).expect("scan_directory should succeed");

        // Should only contain keep.txt
        assert_eq!(tree.total_files, 1, "Should have exactly 1 file");
        assert!(
            tree.contains(&PathBuf::from("keep.txt")),
            "Should contain keep.txt"
        );
        assert!(
            !tree.contains(&PathBuf::from("ignore.log")),
            "Should NOT contain ignore.log"
        );
        assert!(
            !tree.contains(&PathBuf::from("debug.log")),
            "Should NOT contain debug.log"
        );
    }

    #[test]
    fn test_scans_hidden_files_by_default() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir.path();

        fs::write(root.join(".env"), "SECRET=1").expect("Failed to create .env");
        fs::write(root.join("visible.txt"), "ok").expect("Failed to create visible.txt");

        let tree =
            scan_directory(root, &Config::default(), None).expect("scan_directory should succeed");

        assert!(
            tree.contains(&PathBuf::from(".env")),
            "Should contain .env by default"
        );
        assert!(
            tree.contains(&PathBuf::from("visible.txt")),
            "Should contain visible.txt"
        );
    }

    #[test]
    fn test_include_overrides_exclude() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir.path();

        fs::write(root.join("important.log"), "keep").expect("Failed to create important.log");
        fs::write(root.join("other.log"), "ignore").expect("Failed to create other.log");
        fs::write(root.join("note.txt"), "keep").expect("Failed to create note.txt");

        let config = Config {
            source: root.to_path_buf(),
            destination: PathBuf::from("/tmp/dest"),
            exclude_patterns: vec!["*.log".to_string()],
            include_patterns: vec!["important.log".to_string()],
            ..Default::default()
        };

        let tree = scan_directory(root, &config, None).expect("scan_directory should succeed");

        assert!(
            tree.contains(&PathBuf::from("important.log")),
            "important.log should be included"
        );
        assert!(
            !tree.contains(&PathBuf::from("other.log")),
            "other.log should remain excluded"
        );
        assert!(
            tree.contains(&PathBuf::from("note.txt")),
            "note.txt should be included"
        );
    }

    #[test]
    fn test_destination_scan_excludes_kopy_trash() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root_path = temp_dir.path();

        fs::create_dir_all(root_path.join(".kopy_trash/snapshot")).expect("Failed to create trash");
        fs::write(root_path.join(".kopy_trash/snapshot/deleted.txt"), b"old").expect("write trash");
        fs::write(root_path.join("regular.txt"), b"keep").expect("write regular");

        let config = Config {
            source: PathBuf::from("/src"),
            destination: root_path.to_path_buf(),
            ..Config::default()
        };

        let tree = scan_directory(root_path, &config, None).expect("scan directory");
        assert!(tree.contains(&PathBuf::from("regular.txt")));
        assert!(!tree.contains(&PathBuf::from(".kopy_trash/snapshot/deleted.txt")));
    }

    #[test]
    fn test_scan_progress_callback() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir.path();

        // Create 5 files
        for i in 1..=5 {
            let filename = format!("file{}.txt", i);
            fs::write(root.join(&filename), format!("content {}", i))
                .unwrap_or_else(|_| panic!("Failed to create {}", filename));
        }

        // Create atomic counter for progress tracking
        let call_count = Arc::new(AtomicU64::new(0));
        let call_count_clone = Arc::clone(&call_count);

        // Create progress callback
        let callback: ProgressCallback = Box::new(move |files, bytes| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            // Verify counts are increasing
            assert!(files > 0, "File count should be positive");
            assert!(bytes > 0, "Byte count should be positive");
        });

        let config = Config::default();
        let tree =
            scan_directory(root, &config, Some(&callback)).expect("scan_directory should succeed");

        // Verify callback was called 5 times (once per file)
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            5,
            "Progress callback should be called 5 times"
        );

        // Verify tree has correct file count
        assert_eq!(tree.total_files, 5, "Tree should contain 5 files");
    }
}
