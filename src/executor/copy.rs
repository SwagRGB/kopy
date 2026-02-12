//! Atomic file copy implementation

use crate::types::KopyError;
use crate::Config;
use std::fs::{self, File};
use std::io::{Error, ErrorKind, Read, Write};
use std::path::Path;

/// Copy a file atomically using the write-then-rename strategy
///
/// This implements Algorithm 3 from implementation_plan.md:
/// 1. Write to temporary `.part` file
/// 2. Flush and sync to disk
/// 3. Preserve metadata (permissions, mtime)
/// 4. Atomic rename to final destination
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
    let part_path = dest.with_extension("part");
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

    // Partial write recovery for Phase 1:
    // if copy fails after creating a .part file, clean it up so failed runs don't leave junk.
    if copy_result.is_err() && part_path.exists() {
        let _ = fs::remove_file(&part_path);
    }

    copy_result
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
}
