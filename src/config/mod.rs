//! Configuration management

use super::types::DeleteMode;
use clap::Parser;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════
// CLI Argument Parsing
// ═══════════════════════════════════════════════════════════

/// kopy - Modern file synchronization tool
#[derive(Parser, Debug)]
#[command(name = "kopy")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Source directory
    pub source: PathBuf,

    /// Destination directory
    pub destination: PathBuf,

    /// Perform a dry run (show what would be done without executing)
    #[arg(long, short = 'n')]
    pub dry_run: bool,

    /// Enable checksum mode (verify content, not just metadata)
    #[arg(long, short = 'c')]
    pub checksum: bool,

    /// Delete files in destination that don't exist in source (moves to trash)
    #[arg(long, conflicts_with = "delete_permanent")]
    pub delete: bool,

    /// Permanently delete files (DANGEROUS - no trash)
    #[arg(long, conflicts_with = "delete")]
    pub delete_permanent: bool,

    /// Exclude patterns (can be specified multiple times)
    #[arg(long, short = 'e')]
    pub exclude: Vec<String>,

    /// Include patterns (can be specified multiple times)
    #[arg(long, short = 'i')]
    pub include: Vec<String>,
}

// ═══════════════════════════════════════════════════════════
// Configuration Struct
// ═══════════════════════════════════════════════════════════


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
    pub exclude_patterns: Vec<String>,

    /// Include patterns (overrides excludes)
    pub include_patterns: Vec<String>,

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
            exclude_patterns: Vec::new(),
            include_patterns: Vec::new(),
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
    ///
    /// Ensures:
    /// - Source path exists and is a directory
    /// - Source and destination are different paths
    /// - All exclude and include patterns are valid glob patterns
    pub fn validate(&self) -> Result<(), super::types::KopyError> {
        // 1. Check source exists
        if !self.source.exists() {
            return Err(super::types::KopyError::Config(format!(
                "Source path does not exist: {:?}",
                self.source
            )));
        }

        // 2. Check source is a directory
        if !self.source.is_dir() {
            return Err(super::types::KopyError::Config(format!(
                "Source path must be a directory: {:?}",
                self.source
            )));
        }

        // 3. Check source != destination (prevent infinite recursion)
        if self.source == self.destination {
            return Err(super::types::KopyError::Config(
                "Source and destination cannot be the same".to_string(),
            ));
        }

        // 4. Validate exclude patterns are valid globs
        for pattern in &self.exclude_patterns {
            glob::Pattern::new(pattern).map_err(|e| {
                super::types::KopyError::Config(format!(
                    "Invalid exclude pattern '{}': {}",
                    pattern, e
                ))
            })?;
        }

        // 5. Validate include patterns are valid globs
        for pattern in &self.include_patterns {
            glob::Pattern::new(pattern).map_err(|e| {
                super::types::KopyError::Config(format!(
                    "Invalid include pattern '{}': {}",
                    pattern, e
                ))
            })?;
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// CLI → Config Conversion
// ═══════════════════════════════════════════════════════════

impl TryFrom<Cli> for Config {
    type Error = super::types::KopyError;

    /// Convert CLI arguments to Config
    ///
    /// This performs the following mappings:
    /// - `source` and `destination` are copied directly
    /// - `dry_run` and `checksum` flags are copied directly
    /// - Delete mode is determined by flags:
    ///   - `--delete-permanent` → `DeleteMode::Permanent`
    ///   - `--delete` → `DeleteMode::Trash`
    ///   - Neither → `DeleteMode::None`
    /// - `exclude` → `exclude_patterns`
    /// - `include` → `include_patterns`
    ///
    /// The resulting Config is validated before being returned.
    fn try_from(cli: Cli) -> Result<Self, Self::Error> {
        // Determine delete mode based on flags
        let delete_mode = if cli.delete_permanent {
            DeleteMode::Permanent
        } else if cli.delete {
            DeleteMode::Trash
        } else {
            DeleteMode::None
        };

        let config = Config {
            source: cli.source,
            destination: cli.destination,
            dry_run: cli.dry_run,
            checksum_mode: cli.checksum,
            delete_mode,
            exclude_patterns: cli.exclude,
            include_patterns: cli.include,
            ..Default::default() // Use defaults for threads, bandwidth_limit, etc.
        };

        // Validate the config before returning
        config.validate()?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: Create a temporary directory for testing
    fn create_temp_dir() -> TempDir {
        tempfile::tempdir().expect("Failed to create temp dir")
    }

    /// Helper: Create a temporary file for testing
    fn create_temp_file(dir: &TempDir, name: &str) -> PathBuf {
        let file_path = dir.path().join(name);
        fs::write(&file_path, b"test content").expect("Failed to create temp file");
        file_path
    }

    // ═══════════════════════════════════════════════════════════
    // Test 1: Default Config has safe defaults
    // ═══════════════════════════════════════════════════════════
    #[test]
    fn test_default_config() {
        let config = Config::default();

        // Verify safe defaults
        assert_eq!(config.delete_mode, DeleteMode::None);
        assert!(!config.dry_run);
        assert!(!config.checksum_mode);
        assert!(config.exclude_patterns.is_empty());
        assert!(config.include_patterns.is_empty());
    }

    // ═══════════════════════════════════════════════════════════
    // Test 2: Validation fails when source == destination
    // ═══════════════════════════════════════════════════════════
    #[test]
    fn test_validation_fail_same_path() {
        let temp_dir = create_temp_dir();
        let path = temp_dir.path().to_path_buf();

        let config = Config {
            source: path.clone(),
            destination: path,
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("cannot be the same"));
        } else {
            panic!("Expected Config error");
        }
    }

    // ═══════════════════════════════════════════════════════════
    // Test 3: Validation fails when source doesn't exist
    // ═══════════════════════════════════════════════════════════
    #[test]
    fn test_validation_source_not_exists() {
        let config = Config {
            source: PathBuf::from("/nonexistent/path/that/does/not/exist"),
            destination: PathBuf::from("/some/other/path"),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("does not exist"));
        } else {
            panic!("Expected Config error");
        }
    }

    // ═══════════════════════════════════════════════════════════
    // Test 4: Validation fails when source is a file (not directory)
    // ═══════════════════════════════════════════════════════════
    #[test]
    fn test_validation_source_not_directory() {
        let temp_dir = create_temp_dir();
        let file_path = create_temp_file(&temp_dir, "test.txt");
        let dest_dir = create_temp_dir();

        let config = Config {
            source: file_path,
            destination: dest_dir.path().to_path_buf(),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("must be a directory"));
        } else {
            panic!("Expected Config error");
        }
    }

    // ═══════════════════════════════════════════════════════════
    // Test 5: Validation fails with invalid exclude glob pattern
    // ═══════════════════════════════════════════════════════════
    #[test]
    fn test_validation_invalid_glob_exclude() {
        let src_dir = create_temp_dir();
        let dest_dir = create_temp_dir();

        let config = Config {
            source: src_dir.path().to_path_buf(),
            destination: dest_dir.path().to_path_buf(),
            exclude_patterns: vec!["[invalid".to_string()], // Invalid glob pattern
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("Invalid exclude pattern"));
        } else {
            panic!("Expected Config error");
        }
    }

    // ═══════════════════════════════════════════════════════════
    // Test 6: Validation fails with invalid include glob pattern
    // ═══════════════════════════════════════════════════════════
    #[test]
    fn test_validation_invalid_glob_include() {
        let src_dir = create_temp_dir();
        let dest_dir = create_temp_dir();

        let config = Config {
            source: src_dir.path().to_path_buf(),
            destination: dest_dir.path().to_path_buf(),
            include_patterns: vec!["**[".to_string()], // Invalid glob pattern
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("Invalid include pattern"));
        } else {
            panic!("Expected Config error");
        }
    }

    // ═══════════════════════════════════════════════════════════
    // Test 7: Validation succeeds with valid configuration
    // ═══════════════════════════════════════════════════════════
    #[test]
    fn test_validation_success() {
        let src_dir = create_temp_dir();
        let dest_dir = create_temp_dir();

        let config = Config {
            source: src_dir.path().to_path_buf(),
            destination: dest_dir.path().to_path_buf(),
            exclude_patterns: vec!["*.tmp".to_string(), "node_modules/".to_string()],
            include_patterns: vec!["*.rs".to_string(), "Cargo.toml".to_string()],
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_ok());
    }

    // ═══════════════════════════════════════════════════════════
    // CLI → Config Conversion Tests
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_cli_conversion_with_delete() {
        let src_dir = create_temp_dir();
        let dest_dir = create_temp_dir();

        let cli = Cli {
            source: src_dir.path().to_path_buf(),
            destination: dest_dir.path().to_path_buf(),
            dry_run: false,
            checksum: false,
            delete: true,
            delete_permanent: false,
            exclude: vec!["*.tmp".to_string()],
            include: vec!["*.rs".to_string()],
        };

        let config = Config::try_from(cli).unwrap();

        assert_eq!(config.delete_mode, DeleteMode::Trash);
        assert_eq!(config.exclude_patterns, vec!["*.tmp"]);
        assert_eq!(config.include_patterns, vec!["*.rs"]);
        assert!(!config.dry_run);
        assert!(!config.checksum_mode);
    }

    #[test]
    fn test_cli_conversion_with_delete_permanent() {
        let src_dir = create_temp_dir();
        let dest_dir = create_temp_dir();

        let cli = Cli {
            source: src_dir.path().to_path_buf(),
            destination: dest_dir.path().to_path_buf(),
            dry_run: false,
            checksum: false,
            delete: false,
            delete_permanent: true,
            exclude: vec![],
            include: vec![],
        };

        let config = Config::try_from(cli).unwrap();

        assert_eq!(config.delete_mode, DeleteMode::Permanent);
    }

    #[test]
    fn test_cli_conversion_no_delete() {
        let src_dir = create_temp_dir();
        let dest_dir = create_temp_dir();

        let cli = Cli {
            source: src_dir.path().to_path_buf(),
            destination: dest_dir.path().to_path_buf(),
            dry_run: false,
            checksum: false,
            delete: false,
            delete_permanent: false,
            exclude: vec![],
            include: vec![],
        };

        let config = Config::try_from(cli).unwrap();

        assert_eq!(config.delete_mode, DeleteMode::None);
    }

    #[test]
    fn test_cli_conversion_with_checksum() {
        let src_dir = create_temp_dir();
        let dest_dir = create_temp_dir();

        let cli = Cli {
            source: src_dir.path().to_path_buf(),
            destination: dest_dir.path().to_path_buf(),
            dry_run: true,
            checksum: true,
            delete: false,
            delete_permanent: false,
            exclude: vec![],
            include: vec![],
        };

        let config = Config::try_from(cli).unwrap();

        assert!(config.checksum_mode);
        assert!(config.dry_run);
    }

    #[test]
    fn test_cli_conversion_validation_failure() {
        // Non-existent source path should fail validation
        let cli = Cli {
            source: PathBuf::from("/nonexistent/path/that/does/not/exist"),
            destination: PathBuf::from("/some/other/path"),
            dry_run: false,
            checksum: false,
            delete: false,
            delete_permanent: false,
            exclude: vec![],
            include: vec![],
        };

        let result = Config::try_from(cli);
        assert!(result.is_err());

        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("does not exist"));
        } else {
            panic!("Expected Config error");
        }
    }
}
