//! FileEntry - Represents a single file in the sync tree

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// Represents a file in the sync tree
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    /// Relative path from sync root
    pub path: PathBuf,

    /// File size in bytes
    pub size: u64,

    /// Last modification time (UTC)
    pub mtime: SystemTime,

    /// Unix permissions (mode bits)
    pub permissions: u32,

    /// Blake3 content hash (computed lazily)
    pub hash: Option<[u8; 32]>,

    /// Symlink metadata
    pub is_symlink: bool,
    pub symlink_target: Option<PathBuf>,
}

impl FileEntry {
    /// Create a new FileEntry with the given parameters
    pub fn new(
        path: PathBuf,
        size: u64,
        mtime: SystemTime,
        permissions: u32,
    ) -> Self {
        Self {
            path,
            size,
            mtime,
            permissions,
            hash: None,
            is_symlink: false,
            symlink_target: None,
        }
    }

    /// Create a new FileEntry for a symlink
    pub fn new_symlink(
        path: PathBuf,
        size: u64,
        mtime: SystemTime,
        permissions: u32,
        target: PathBuf,
    ) -> Self {
        Self {
            path,
            size,
            mtime,
            permissions,
            hash: None,
            is_symlink: true,
            symlink_target: Some(target),
        }
    }

    /// Set the hash for this file entry
    pub fn with_hash(mut self, hash: [u8; 32]) -> Self {
        self.hash = Some(hash);
        self
    }

    /// Check if this entry has a computed hash
    pub fn has_hash(&self) -> bool {
        self.hash.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn test_new_file_entry() {
        let path = PathBuf::from("test/file.txt");
        let size = 1024;
        let mtime = UNIX_EPOCH + Duration::from_secs(1000);
        let permissions = 0o644;

        let entry = FileEntry::new(path.clone(), size, mtime, permissions);

        assert_eq!(entry.path, path);
        assert_eq!(entry.size, size);
        assert_eq!(entry.mtime, mtime);
        assert_eq!(entry.permissions, permissions);
        assert_eq!(entry.hash, None);
        assert!(!entry.is_symlink);
        assert_eq!(entry.symlink_target, None);
    }

    #[test]
    fn test_new_symlink_entry() {
        let path = PathBuf::from("test/link.txt");
        let target = PathBuf::from("test/target.txt");
        let size = 0;
        let mtime = UNIX_EPOCH + Duration::from_secs(2000);
        let permissions = 0o777;

        let entry = FileEntry::new_symlink(path.clone(), size, mtime, permissions, target.clone());

        assert_eq!(entry.path, path);
        assert_eq!(entry.size, size);
        assert_eq!(entry.mtime, mtime);
        assert_eq!(entry.permissions, permissions);
        assert!(entry.is_symlink);
        assert_eq!(entry.symlink_target, Some(target));
        assert_eq!(entry.hash, None);
    }

    #[test]
    fn test_with_hash() {
        let path = PathBuf::from("test/file.txt");
        let size = 2048;
        let mtime = UNIX_EPOCH + Duration::from_secs(3000);
        let permissions = 0o755;
        let hash = [42u8; 32];

        let entry = FileEntry::new(path, size, mtime, permissions).with_hash(hash);

        assert_eq!(entry.hash, Some(hash));
        assert!(entry.has_hash());
    }

    #[test]
    fn test_has_hash_returns_false_when_no_hash() {
        let path = PathBuf::from("test/file.txt");
        let size = 512;
        let mtime = UNIX_EPOCH + Duration::from_secs(4000);
        let permissions = 0o600;

        let entry = FileEntry::new(path, size, mtime, permissions);

        assert!(!entry.has_hash());
    }

    #[test]
    fn test_serialization() {
        let path = PathBuf::from("test/serialize.txt");
        let size = 4096;
        let mtime = UNIX_EPOCH + Duration::from_secs(5000);
        let permissions = 0o644;
        let hash = [1u8; 32];

        let entry = FileEntry::new(path, size, mtime, permissions).with_hash(hash);

        // Serialize to JSON
        let serialized = serde_json::to_string(&entry).expect("Failed to serialize");

        // Deserialize back
        let deserialized: FileEntry =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(entry, deserialized);
    }

    #[test]
    fn test_clone() {
        let path = PathBuf::from("test/clone.txt");
        let size = 8192;
        let mtime = UNIX_EPOCH + Duration::from_secs(6000);
        let permissions = 0o755;

        let entry = FileEntry::new(path.clone(), size, mtime, permissions);
        let cloned = entry.clone();

        assert_eq!(entry, cloned);
        assert_eq!(entry.path, cloned.path);
        assert_eq!(entry.size, cloned.size);
    }

    #[test]
    fn test_symlink_serialization() {
        let path = PathBuf::from("test/link");
        let target = PathBuf::from("test/real_file.txt");
        let size = 0;
        let mtime = UNIX_EPOCH + Duration::from_secs(7000);
        let permissions = 0o777;

        let entry = FileEntry::new_symlink(path, size, mtime, permissions, target);

        // Serialize to JSON
        let serialized = serde_json::to_string(&entry).expect("Failed to serialize symlink");

        // Deserialize back
        let deserialized: FileEntry =
            serde_json::from_str(&serialized).expect("Failed to deserialize symlink");

        assert_eq!(entry, deserialized);
        assert!(deserialized.is_symlink);
        assert!(deserialized.symlink_target.is_some());
    }

    #[test]
    fn test_large_file_size() {
        let path = PathBuf::from("test/large.bin");
        let size = u64::MAX; // Maximum file size
        let mtime = UNIX_EPOCH + Duration::from_secs(8000);
        let permissions = 0o644;

        let entry = FileEntry::new(path, size, mtime, permissions);

        assert_eq!(entry.size, u64::MAX);
    }

    #[test]
    fn test_zero_size_file() {
        let path = PathBuf::from("test/empty.txt");
        let size = 0;
        let mtime = UNIX_EPOCH + Duration::from_secs(9000);
        let permissions = 0o644;

        let entry = FileEntry::new(path, size, mtime, permissions);

        assert_eq!(entry.size, 0);
    }

    #[test]
    fn test_various_permissions() {
        let test_cases = vec![
            0o000, // No permissions
            0o400, // Read only (owner)
            0o644, // Standard file
            0o755, // Executable
            0o777, // All permissions
        ];

        for perm in test_cases {
            let path = PathBuf::from(format!("test/perm_{:o}.txt", perm));
            let entry = FileEntry::new(path, 100, UNIX_EPOCH, perm);
            assert_eq!(entry.permissions, perm);
        }
    }
}
