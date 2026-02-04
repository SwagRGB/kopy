//! FileTree - Directory structure representation

use super::FileEntry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// File tree (directory structure)
#[derive(Debug, Clone, PartialEq)]
pub struct FileTree {
    /// Map: relative_path → FileEntry
    pub entries: HashMap<PathBuf, FileEntry>,

    /// Aggregate statistics
    pub total_size: u64,
    pub total_files: usize,
    pub total_dirs: usize,

    /// Scan metadata
    pub scan_duration: Duration,
    pub root_path: PathBuf,
}

impl FileTree {
    /// Create a new empty FileTree
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            entries: HashMap::new(),
            total_size: 0,
            total_files: 0,
            total_dirs: 0,
            scan_duration: Duration::from_secs(0),
            root_path,
        }
    }

    /// Insert a file entry into the tree
    /// 
    /// Updates aggregate statistics (total_size, total_files).
    /// If the path already exists, the old entry is replaced and statistics are adjusted.
    pub fn insert(&mut self, path: PathBuf, entry: FileEntry) {
        // If replacing an existing entry, subtract its size first
        if let Some(old_entry) = self.entries.get(&path) {
            self.total_size = self.total_size.saturating_sub(old_entry.size);
            self.total_files = self.total_files.saturating_sub(1);
        }

        self.total_size += entry.size;
        self.total_files += 1;
        self.entries.insert(path, entry);
    }

    /// Get a file entry by path
    pub fn get(&self, path: &PathBuf) -> Option<&FileEntry> {
        self.entries.get(path)
    }

    /// Check if a path exists in the tree
    pub fn contains(&self, path: &PathBuf) -> bool {
        self.entries.contains_key(path)
    }

    /// Return the number of file entries in the tree
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterator over all entries (path, FileEntry pairs)
    pub fn iter(&self) -> impl Iterator<Item = (&PathBuf, &FileEntry)> {
        self.entries.iter()
    }

    /// Iterator over just the paths
    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.entries.keys()
    }

    /// Set the scan duration after scanning completes
    pub fn set_scan_duration(&mut self, duration: Duration) {
        self.scan_duration = duration;
    }

    /// Increment the directory counter
    /// 
    /// Called during directory scanning to track the number of directories traversed
    pub fn increment_dirs(&mut self) {
        self.total_dirs += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn create_test_entry(name: &str, size: u64) -> FileEntry {
        FileEntry::new(
            PathBuf::from(name),
            size,
            UNIX_EPOCH + Duration::from_secs(1000),
            0o644,
        )
    }

    #[test]
    fn test_new_tree() {
        let root = PathBuf::from("/test/root");
        let tree = FileTree::new(root.clone());

        assert_eq!(tree.root_path, root);
        assert_eq!(tree.total_size, 0);
        assert_eq!(tree.total_files, 0);
        assert_eq!(tree.total_dirs, 0);
        assert_eq!(tree.scan_duration, Duration::from_secs(0));
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
    }

    #[test]
    fn test_insert_single_entry() {
        let mut tree = FileTree::new(PathBuf::from("/root"));
        let path = PathBuf::from("file.txt");
        let entry = create_test_entry("file.txt", 1024);

        tree.insert(path.clone(), entry.clone());

        assert_eq!(tree.len(), 1);
        assert_eq!(tree.total_files, 1);
        assert_eq!(tree.total_size, 1024);
        assert!(tree.contains(&path));
        assert_eq!(tree.get(&path), Some(&entry));
    }

    #[test]
    fn test_insert_multiple_entries() {
        let mut tree = FileTree::new(PathBuf::from("/root"));

        let files = vec![
            ("file1.txt", 100),
            ("file2.txt", 200),
            ("dir/file3.txt", 300),
        ];

        for (name, size) in &files {
            let path = PathBuf::from(name);
            let entry = create_test_entry(name, *size);
            tree.insert(path, entry);
        }

        assert_eq!(tree.len(), 3);
        assert_eq!(tree.total_files, 3);
        assert_eq!(tree.total_size, 600);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_get_existing_entry() {
        let mut tree = FileTree::new(PathBuf::from("/root"));
        let path = PathBuf::from("test.txt");
        let entry = create_test_entry("test.txt", 512);

        tree.insert(path.clone(), entry.clone());

        let retrieved = tree.get(&path);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &entry);
    }

    #[test]
    fn test_get_nonexistent_entry() {
        let tree = FileTree::new(PathBuf::from("/root"));
        let path = PathBuf::from("nonexistent.txt");

        assert_eq!(tree.get(&path), None);
    }

    #[test]
    fn test_contains() {
        let mut tree = FileTree::new(PathBuf::from("/root"));
        let path1 = PathBuf::from("exists.txt");
        let path2 = PathBuf::from("not_exists.txt");

        tree.insert(path1.clone(), create_test_entry("exists.txt", 100));

        assert!(tree.contains(&path1));
        assert!(!tree.contains(&path2));
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut tree = FileTree::new(PathBuf::from("/root"));

        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);

        tree.insert(PathBuf::from("file1.txt"), create_test_entry("file1.txt", 100));
        assert!(!tree.is_empty());
        assert_eq!(tree.len(), 1);

        tree.insert(PathBuf::from("file2.txt"), create_test_entry("file2.txt", 200));
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn test_iteration() {
        let mut tree = FileTree::new(PathBuf::from("/root"));

        let files = vec![
            ("a.txt", 100),
            ("b.txt", 200),
            ("c.txt", 300),
        ];

        for (name, size) in &files {
            tree.insert(PathBuf::from(name), create_test_entry(name, *size));
        }

        // Test iter()
        let count = tree.iter().count();
        assert_eq!(count, 3);

        // Test paths()
        let paths: Vec<_> = tree.paths().collect();
        assert_eq!(paths.len(), 3);

        // Verify all paths are present
        for (name, _) in &files {
            let path = PathBuf::from(name);
            assert!(paths.contains(&&path));
        }
    }

    #[test]
    fn test_duplicate_insertion() {
        let mut tree = FileTree::new(PathBuf::from("/root"));
        let path = PathBuf::from("file.txt");

        // Insert first version
        let entry1 = create_test_entry("file.txt", 1000);
        tree.insert(path.clone(), entry1);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree.total_files, 1);
        assert_eq!(tree.total_size, 1000);

        // Insert second version (same path, different size)
        let entry2 = create_test_entry("file.txt", 2000);
        tree.insert(path.clone(), entry2.clone());

        // Should still have 1 entry, but updated statistics
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.total_files, 1);
        assert_eq!(tree.total_size, 2000); // Updated size
        assert_eq!(tree.get(&path), Some(&entry2));
    }

    #[test]
    fn test_scan_duration() {
        let mut tree = FileTree::new(PathBuf::from("/root"));

        assert_eq!(tree.scan_duration, Duration::from_secs(0));

        let duration = Duration::from_millis(1500);
        tree.set_scan_duration(duration);

        assert_eq!(tree.scan_duration, duration);
    }

    #[test]
    fn test_directory_counting() {
        let mut tree = FileTree::new(PathBuf::from("/root"));

        assert_eq!(tree.total_dirs, 0);

        tree.increment_dirs();
        assert_eq!(tree.total_dirs, 1);

        tree.increment_dirs();
        tree.increment_dirs();
        assert_eq!(tree.total_dirs, 3);
    }

    #[test]
    fn test_large_tree() {
        let mut tree = FileTree::new(PathBuf::from("/root"));

        // Insert 1000 files
        for i in 0..1000 {
            let path = PathBuf::from(format!("file_{}.txt", i));
            let entry = create_test_entry(&format!("file_{}.txt", i), 1024);
            tree.insert(path, entry);
        }

        assert_eq!(tree.len(), 1000);
        assert_eq!(tree.total_files, 1000);
        assert_eq!(tree.total_size, 1024 * 1000);
    }

    #[test]
    fn test_zero_size_files() {
        let mut tree = FileTree::new(PathBuf::from("/root"));

        tree.insert(PathBuf::from("empty.txt"), create_test_entry("empty.txt", 0));
        tree.insert(PathBuf::from("also_empty.txt"), create_test_entry("also_empty.txt", 0));

        assert_eq!(tree.len(), 2);
        assert_eq!(tree.total_files, 2);
        assert_eq!(tree.total_size, 0);
    }

    #[test]
    fn test_clone() {
        let mut tree = FileTree::new(PathBuf::from("/root"));
        tree.insert(PathBuf::from("file.txt"), create_test_entry("file.txt", 500));
        tree.set_scan_duration(Duration::from_secs(5));
        tree.increment_dirs();

        let cloned = tree.clone();

        assert_eq!(tree, cloned);
        assert_eq!(cloned.len(), 1);
        assert_eq!(cloned.total_size, 500);
        assert_eq!(cloned.scan_duration, Duration::from_secs(5));
        assert_eq!(cloned.total_dirs, 1);
    }
}
