//! Atomic file copy implementation

use crate::types::KopyError;
use crate::Config;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{Error, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COPY_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Copy a file atomically using the write-then-rename strategy
///
/// Data is written to a temporary `.part` file, synced, metadata is copied, and
/// then renamed into place.
///
/// # Arguments
/// * `src` - Source file path
/// * `dest` - Destination file path
/// * `config` - Configuration (for future bandwidth limiting, etc.)
///
/// # Returns
/// * `Ok(u64)` - Number of bytes copied
/// * `Err(KopyError)` - IO error or other failure
///
/// # Example
/// ```no_run
/// use kopy::executor::copy_file_atomic;
/// use kopy::Config;
/// use std::path::Path;
///
/// let config = Config::default();
/// let bytes = copy_file_atomic(
///     Path::new("source.txt"),
///     Path::new("dest.txt"),
///     &config
/// )?;
/// # Ok::<(), kopy::types::KopyError>(())
/// ```
pub fn copy_file_atomic(src: &Path, dest: &Path, _config: &Config) -> Result<u64, KopyError> {
    let part_path = build_temp_path(dest);
    let copy_result = (|| -> Result<u64, KopyError> {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| map_file_error(parent, e))?;
        }

        let mut src_file = File::open(src).map_err(|e| map_file_error(src, e))?;
        let mut part_file = File::create(&part_path).map_err(|e| map_file_error(dest, e))?;

        let mut buffer = vec![0u8; 128 * 1024];
        let mut total_bytes = 0u64;

        loop {
            let bytes_read = src_file
                .read(&mut buffer)
                .map_err(|e| map_file_error(src, e))?;

            if bytes_read == 0 {
                break; // EOF
            }

            part_file
                .write_all(&buffer[0..bytes_read])
                .map_err(|e| map_file_error(dest, e))?;
            total_bytes += bytes_read as u64;
        }

        part_file.sync_all().map_err(|e| map_file_error(dest, e))?;

        drop(part_file);

        let src_metadata = fs::metadata(src).map_err(|e| map_file_error(src, e))?;

        fs::set_permissions(&part_path, src_metadata.permissions())
            .map_err(|e| map_file_error(dest, e))?;

        let mtime = src_metadata
            .modified()
            .map_err(|e| map_file_error(src, e))?;
        let filetime_mtime = filetime::FileTime::from_system_time(mtime);
        filetime::set_file_mtime(&part_path, filetime_mtime)
            .map_err(|e| map_file_error(dest, e))?;

        fs::rename(&part_path, dest).map_err(|e| map_file_error(dest, e))?;

        Ok(total_bytes)
    })();

    // Remove partially written temp file on failure.
    if copy_result.is_err() && part_path.exists() {
        let _ = fs::remove_file(&part_path);
    }

    copy_result
}

fn build_temp_path(dest: &Path) -> PathBuf {
    let basename = dest.file_name().unwrap_or_else(|| OsStr::new("kopy_tmp"));
    let unique = COPY_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);

    let mut temp_name = OsString::from(".");
    temp_name.push(basename);
    temp_name.push(".kopy.part.");
    temp_name.push(std::process::id().to_string());
    temp_name.push(".");
    temp_name.push(unique.to_string());

    dest.with_file_name(temp_name)
}

fn map_file_error(path: &Path, error: Error) -> KopyError {
    if is_permission_error(&error) {
        KopyError::PermissionDenied {
            path: path.to_path_buf(),
        }
    } else if is_disk_full_error(&error) {
        KopyError::DiskFull {
            available: 0,
            needed: 1,
        }
    } else {
        KopyError::Io(error)
    }
}

fn is_permission_error(error: &Error) -> bool {
    matches!(error.kind(), ErrorKind::PermissionDenied)
}

fn is_disk_full_error(error: &Error) -> bool {
    matches!(error.kind(), ErrorKind::StorageFull) || matches!(error.raw_os_error(), Some(28 | 122))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_map_permission_error() {
        let error = Error::from(ErrorKind::PermissionDenied);
        let mapped = map_file_error(Path::new("file.txt"), error);
        assert!(matches!(mapped, KopyError::PermissionDenied { .. }));
    }

    #[test]
    fn test_map_disk_full_error_kind() {
        let error = Error::from(ErrorKind::StorageFull);
        let mapped = map_file_error(Path::new("file.txt"), error);
        assert!(matches!(mapped, KopyError::DiskFull { .. }));
    }

    #[test]
    fn test_map_io_fallback() {
        let error = Error::from(ErrorKind::NotFound);
        let mapped = map_file_error(Path::new("file.txt"), error);
        assert!(matches!(mapped, KopyError::Io(_)));
    }

    #[test]
    fn test_copy_file_atomic_basic_content() {
        let temp = TempDir::new().expect("create temp dir");
        let src = temp.path().join("src.txt");
        let dest = temp.path().join("dest.txt");

        fs::write(&src, b"hello copy").expect("write src");
        let config = Config::default();

        let copied = copy_file_atomic(&src, &dest, &config).expect("copy");
        assert_eq!(copied, 10);
        assert_eq!(fs::read(&dest).expect("read dest"), b"hello copy");
    }

    #[test]
    fn test_copy_file_atomic_creates_parent_directories() {
        let temp = TempDir::new().expect("create temp dir");
        let src = temp.path().join("src.txt");
        let dest = temp.path().join("a/b/c/dest.txt");

        fs::write(&src, b"nested").expect("write src");
        let config = Config::default();

        copy_file_atomic(&src, &dest, &config).expect("copy");
        assert!(dest.exists());
        assert_eq!(fs::read(&dest).expect("read dest"), b"nested");
    }

    #[test]
    fn test_copy_file_atomic_for_part_extension_destination() {
        let temp = TempDir::new().expect("create temp dir");
        let src = temp.path().join("src.part");
        let dest = temp.path().join("dest.part");

        fs::write(&src, b"part-bytes").expect("write src");
        let config = Config::default();

        copy_file_atomic(&src, &dest, &config).expect("copy");
        assert_eq!(fs::read(&dest).expect("read dest"), b"part-bytes");
    }

    #[test]
    fn test_copy_file_atomic_does_not_clobber_sibling_part_file() {
        let temp = TempDir::new().expect("create temp dir");
        let src = temp.path().join("source.txt");
        let dest = temp.path().join("target");
        let sibling_part = temp.path().join("target.part");

        fs::write(&src, b"fresh").expect("write src");
        fs::write(&sibling_part, b"keep-me").expect("write sibling");
        let config = Config::default();

        copy_file_atomic(&src, &dest, &config).expect("copy");
        assert_eq!(fs::read(&dest).expect("read dest"), b"fresh");
        assert_eq!(fs::read(&sibling_part).expect("read sibling"), b"keep-me");
    }
}
