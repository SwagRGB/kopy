//! Hashing utilities

use crate::types::KopyError;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Compute Blake3 hash of a file
///
/// This implements the lazy hash computation algorithm from implementation_plan.md.
/// The file is streamed in 64KB chunks for memory efficiency.
///
/// # Arguments
/// * `file_path` - Path to the file to hash
///
/// # Returns
/// * `Ok([u8; 32])` - 32-byte Blake3 hash
/// * `Err(KopyError)` - IO error if file cannot be read
///
/// # Example
/// ```no_run
/// use kopy::hash::compute_hash;
/// use std::path::Path;
///
/// let hash = compute_hash(Path::new("file.txt"))?;
/// # Ok::<(), kopy::types::KopyError>(())
/// ```
pub fn compute_hash(file_path: &Path) -> Result<[u8; 32], KopyError> {
    // Open file for reading
    let mut file = File::open(file_path).map_err(KopyError::Io)?;

    // Create Blake3 hasher
    let mut hasher = blake3::Hasher::new();

    // Stream file in 64KB chunks (memory efficient)
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        let bytes_read = file.read(&mut buffer).map_err(KopyError::Io)?;

        if bytes_read == 0 {
            break; // EOF
        }

        hasher.update(&buffer[0..bytes_read]);
    }

    // Finalize and return hash
    let hash = hasher.finalize();
    Ok(*hash.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_hash_empty_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"").unwrap();
        temp_file.flush().unwrap();

        let hash = compute_hash(temp_file.path()).unwrap();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_hash_deterministic() {
        let content = b"Test content for hashing";

        let mut file1 = NamedTempFile::new().unwrap();
        file1.write_all(content).unwrap();
        file1.flush().unwrap();

        let mut file2 = NamedTempFile::new().unwrap();
        file2.write_all(content).unwrap();
        file2.flush().unwrap();

        let hash1 = compute_hash(file1.path()).unwrap();
        let hash2 = compute_hash(file2.path()).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_different_content() {
        let mut file1 = NamedTempFile::new().unwrap();
        file1.write_all(b"Content A").unwrap();
        file1.flush().unwrap();

        let mut file2 = NamedTempFile::new().unwrap();
        file2.write_all(b"Content B").unwrap();
        file2.flush().unwrap();

        let hash1 = compute_hash(file1.path()).unwrap();
        let hash2 = compute_hash(file2.path()).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_nonexistent_file() {
        let path = Path::new("/nonexistent/file.txt");
        let result = compute_hash(path);

        assert!(result.is_err());
    }
}
