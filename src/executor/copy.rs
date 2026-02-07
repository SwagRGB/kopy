//! Atomic file copy implementation

use crate::types::KopyError;
use crate::Config;
use std::fs::{self, File};
use std::io::{Read, Write};
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
    // ═══════════════════════════════════════════════════════════
    // STEP 1: Prepare - Create parent directories and .part path
    // ═══════════════════════════════════════════════════════════
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(KopyError::Io)?;
    }

    let part_path = dest.with_extension("part");

    // ═══════════════════════════════════════════════════════════
    // STEP 2: Copy - Stream from src to .part file
    // ═══════════════════════════════════════════════════════════
    let mut src_file = File::open(src).map_err(KopyError::Io)?;
    let mut part_file = File::create(&part_path).map_err(KopyError::Io)?;

    // Use 128KB buffer as specified in implementation_plan.md
    let mut buffer = vec![0u8; 128 * 1024];
    let mut total_bytes = 0u64;

    loop {
        let bytes_read = src_file.read(&mut buffer).map_err(KopyError::Io)?;

        if bytes_read == 0 {
            break; // EOF
        }

        part_file
            .write_all(&buffer[0..bytes_read])
            .map_err(KopyError::Io)?;
        total_bytes += bytes_read as u64;
    }

    // ═══════════════════════════════════════════════════════════
    // STEP 3: Flush - Force OS to write data to physical disk
    // ═══════════════════════════════════════════════════════════
    part_file.sync_all().map_err(KopyError::Io)?;

    // Drop the file handle before rename (required on Windows)
    drop(part_file);

    // ═══════════════════════════════════════════════════════════
    // STEP 4: Metadata - Preserve permissions and mtime
    // ═══════════════════════════════════════════════════════════
    let src_metadata = fs::metadata(src).map_err(KopyError::Io)?;

    // Copy permissions
    fs::set_permissions(&part_path, src_metadata.permissions()).map_err(KopyError::Io)?;

    // Copy modification time
    let mtime = src_metadata.modified().map_err(KopyError::Io)?;
    let filetime_mtime = filetime::FileTime::from_system_time(mtime);
    filetime::set_file_mtime(&part_path, filetime_mtime).map_err(KopyError::Io)?;

    // ═══════════════════════════════════════════════════════════
    // STEP 5: Commit - Atomic rename to final destination
    // ═══════════════════════════════════════════════════════════
    // This is atomic on POSIX systems (single syscall)
    fs::rename(&part_path, dest).map_err(KopyError::Io)?;

    Ok(total_bytes)
}
