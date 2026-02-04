//! FileTree - Directory structure representation

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use super::FileEntry;

/// File tree (directory structure)
#[derive(Debug, Clone)]
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
    pub fn insert(&mut self, path: PathBuf, entry: FileEntry) {
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
}
