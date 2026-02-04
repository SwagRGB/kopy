//! Configuration management

use super::types::DeleteMode;
use std::path::PathBuf;

/// Global configuration for kopy
#[derive(Debug, Clone)]
pub struct Config {
    /// Source directory
    pub source: PathBuf,

    /// Destination directory
    pub destination: PathBuf,

    /// Dry run (show plan, don't execute)
    pub dry_run: bool,

    /// Force checksum verification (slow but paranoid)
    pub checksum_mode: bool,

    /// How to handle deletes
    pub delete_mode: DeleteMode,

    /// Exclude patterns (globs)
    pub exclude: Vec<String>,

    /// Include patterns (overrides excludes)
    pub include: Vec<String>,

    /// Number of worker threads (Phase 2)
    pub threads: usize,

    /// Bandwidth limit (bytes/sec, None = unlimited)
    pub bandwidth_limit: Option<u64>,

    /// Backup directory for snapshots (Phase 3)
    pub backup_dir: Option<PathBuf>,

    /// Watch mode enabled? (Phase 3)
    pub watch: bool,

    /// Watch settle time (seconds)
    pub watch_settle: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: PathBuf::new(),
            destination: PathBuf::new(),
            dry_run: false,
            checksum_mode: false,
            delete_mode: DeleteMode::None,
            exclude: Vec::new(),
            include: Vec::new(),
            threads: 4, // Phase 1: hardcoded, Phase 2: use num_cpus
            bandwidth_limit: None,
            backup_dir: None,
            watch: false,
            watch_settle: 2,
        }
    }
}

impl Config {
    /// Validate configuration
    pub fn validate(&self) -> Result<(), super::types::KopyError> {
        // Ensure source exists
        if !self.source.exists() {
            return Err(super::types::KopyError::Config(format!(
                "Source path does not exist: {:?}",
                self.source
            )));
        }

        // Ensure source != destination
        if self.source == self.destination {
            return Err(super::types::KopyError::Config(
                "Source and destination cannot be the same".to_string(),
            ));
        }

        Ok(())
    }
}
