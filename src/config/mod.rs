//! Configuration management

use super::types::DeleteMode;
use clap::{Parser, ValueEnum};
use std::path::{Component, Path, PathBuf};

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

    /// Scan strategy: auto chooses based on sampled tree shape.
    #[arg(long, value_enum, default_value_t = ScanMode::Auto)]
    pub scan_mode: ScanMode,
}

/// Directory scan execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScanMode {
    /// Choose sequential/parallel based on sampled tree shape.
    Auto,
    /// Force sequential scanner.
    Sequential,
    /// Force parallel scanner.
    Parallel,
}

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

    /// Number of worker threads.
    pub threads: usize,

    /// Directory scan mode.
    pub scan_mode: ScanMode,

    /// Bandwidth limit (bytes/sec, None = unlimited)
    pub bandwidth_limit: Option<u64>,

    /// Backup directory for snapshots.
    pub backup_dir: Option<PathBuf>,

    /// Watch mode enabled.
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
            threads: 4,
            scan_mode: ScanMode::Auto,
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
    /// - Source path exists and is a file or directory
    /// - Source and destination are different paths
    /// - All exclude and include patterns are valid glob patterns
    ///
    /// # Example
    /// ```no_run
    /// use kopy::Config;
    /// use std::path::PathBuf;
    ///
    /// let config = Config {
    ///     source: PathBuf::from("./src_dir"),
    ///     destination: PathBuf::from("./dst_dir"),
    ///     ..Config::default()
    /// };
    ///
    /// config.validate()?;
    /// # Ok::<(), kopy::types::KopyError>(())
    /// ```
    pub fn validate(&self) -> Result<(), super::types::KopyError> {
        // 1. Check source exists
        if !self.source.exists() {
            return Err(super::types::KopyError::Config(format!(
                "Source path does not exist: {:?}",
                self.source
            )));
        }

        let source_is_dir = self.source.is_dir();
        let source_is_file = self.source.is_file();
        if !source_is_dir && !source_is_file {
            return Err(super::types::KopyError::Config(format!(
                "Source path must be a file or directory: {:?}",
                self.source
            )));
        }

        if source_is_dir && self.destination.exists() && !self.destination.is_dir() {
            return Err(super::types::KopyError::Config(format!(
                "Destination path must be a directory if it exists: {:?}",
                self.destination
            )));
        }

        // 3. Check source != destination (prevent infinite recursion)
        if self.source == self.destination {
            return Err(super::types::KopyError::Config(
                "Source and destination cannot be the same".to_string(),
            ));
        }

        // 3.1. Check for nested source/destination roots (prevents recursive growth)
        let source_normalized = canonical_or_normalized(&self.source)?;
        let destination_normalized = canonical_or_normalized(&self.destination)?;
        if source_normalized == destination_normalized {
            return Err(super::types::KopyError::Config(format!(
                "Source and destination cannot resolve to the same directory. source='{}', destination='{}'",
                self.source.display(),
                self.destination.display()
            )));
        }

        if source_is_dir
            && (is_strict_descendant(&destination_normalized, &source_normalized)
                || is_strict_descendant(&source_normalized, &destination_normalized))
        {
            return Err(super::types::KopyError::Config(format!(
                "Source and destination cannot be nested. source='{}', destination='{}'",
                self.source.display(),
                self.destination.display()
            )));
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

fn is_strict_descendant(path: &Path, potential_ancestor: &Path) -> bool {
    path.starts_with(potential_ancestor) && path != potential_ancestor
}

/// Return a canonical path for existing entries, or a normalized absolute path for missing ones.
///
/// This allows nested-path validation to work even when one side does not exist yet.
fn canonical_or_normalized(path: &Path) -> Result<PathBuf, super::types::KopyError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(super::types::KopyError::Io)?
            .join(path)
    };

    if absolute.exists() {
        return absolute.canonicalize().map_err(super::types::KopyError::Io);
    }

    // Resolve symlinked parent components by canonicalizing nearest existing ancestor.
    let mut ancestor = absolute.clone();
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        let Some(name) = ancestor.file_name() else {
            break;
        };
        suffix.push(name.to_os_string());
        if !ancestor.pop() {
            break;
        }
    }

    if ancestor.exists() {
        let mut resolved = ancestor
            .canonicalize()
            .map_err(super::types::KopyError::Io)?;
        for component in suffix.iter().rev() {
            resolved.push(component);
        }
        Ok(normalize_path(&resolved))
    } else {
        Ok(normalize_path(&absolute))
    }
}

/// Normalize `.` and `..` path components without touching filesystem state.
///
/// This is lexical normalization; symlink resolution is intentionally not performed here.
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    normalized
}

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
    ///
    /// # Errors
    /// Returns `KopyError::Config` for invalid path relationships or invalid glob patterns.
    fn try_from(cli: Cli) -> Result<Self, Self::Error> {
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
            scan_mode: cli.scan_mode,
            ..Default::default()
        };

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

    #[test]
    fn test_default_config() {
        let config = Config::default();

        assert_eq!(config.delete_mode, DeleteMode::None);
        assert!(!config.dry_run);
        assert!(!config.checksum_mode);
        assert!(config.exclude_patterns.is_empty());
        assert!(config.include_patterns.is_empty());
        assert_eq!(config.scan_mode, ScanMode::Auto);
    }

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

    #[test]
    fn test_validation_source_file_is_allowed() {
        let temp_dir = create_temp_dir();
        let file_path = create_temp_file(&temp_dir, "test.txt");
        let dest_dir = create_temp_dir();

        let config = Config {
            source: file_path,
            destination: dest_dir.path().to_path_buf(),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validation_source_file_allows_destination_file() {
        let temp_dir = create_temp_dir();
        let file_path = create_temp_file(&temp_dir, "source.txt");
        let destination_file = temp_dir.path().join("renamed.txt");

        let config = Config {
            source: file_path,
            destination: destination_file,
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validation_destination_existing_file_is_rejected() {
        let src_dir = create_temp_dir();
        let dest_dir = create_temp_dir();
        let dest_file = create_temp_file(&dest_dir, "dest.txt");

        let config = Config {
            source: src_dir.path().to_path_buf(),
            destination: dest_file,
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("Destination path must be a directory"));
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_validation_invalid_glob_exclude() {
        let src_dir = create_temp_dir();
        let dest_dir = create_temp_dir();

        let config = Config {
            source: src_dir.path().to_path_buf(),
            destination: dest_dir.path().to_path_buf(),
            exclude_patterns: vec!["[invalid".to_string()],
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

    #[test]
    fn test_validation_invalid_glob_include() {
        let src_dir = create_temp_dir();
        let dest_dir = create_temp_dir();

        let config = Config {
            source: src_dir.path().to_path_buf(),
            destination: dest_dir.path().to_path_buf(),
            include_patterns: vec!["**[".to_string()],
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

    #[test]
    fn test_validation_fail_destination_nested_in_source() {
        let src_dir = create_temp_dir();
        let nested_dest = src_dir.path().join("backup");

        let config = Config {
            source: src_dir.path().to_path_buf(),
            destination: nested_dest,
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("cannot be nested"));
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_validation_fail_source_nested_in_destination() {
        let dest_dir = create_temp_dir();
        let nested_source = dest_dir.path().join("source");
        fs::create_dir_all(&nested_source).expect("Failed to create nested source");

        let config = Config {
            source: nested_source,
            destination: dest_dir.path().to_path_buf(),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("cannot be nested"));
        } else {
            panic!("Expected Config error");
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_validation_fail_canonical_equal_via_symlink_alias() {
        use std::os::unix::fs::symlink;

        let src_dir = create_temp_dir();
        let alias_parent = create_temp_dir();
        let alias_path = alias_parent.path().join("src_alias");
        symlink(src_dir.path(), &alias_path).expect("create symlink alias");

        let config = Config {
            source: src_dir.path().to_path_buf(),
            destination: alias_path,
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("resolve to the same directory"));
        } else {
            panic!("Expected Config error");
        }
    }

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
            scan_mode: ScanMode::Auto,
        };

        let config = Config::try_from(cli).unwrap();

        assert_eq!(config.delete_mode, DeleteMode::Trash);
        assert_eq!(config.exclude_patterns, vec!["*.tmp"]);
        assert_eq!(config.include_patterns, vec!["*.rs"]);
        assert_eq!(config.scan_mode, ScanMode::Auto);
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
            scan_mode: ScanMode::Auto,
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
            scan_mode: ScanMode::Auto,
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
            scan_mode: ScanMode::Auto,
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
            scan_mode: ScanMode::Auto,
        };

        let result = Config::try_from(cli);
        assert!(result.is_err());

        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("does not exist"));
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_cli_parse_scan_mode_default_auto() {
        let cli = Cli::try_parse_from(["kopy", "src", "dst"]).expect("parse cli");
        assert_eq!(cli.scan_mode, ScanMode::Auto);
    }

    #[test]
    fn test_cli_parse_scan_mode_parallel() {
        let cli = Cli::try_parse_from(["kopy", "src", "dst", "--scan-mode", "parallel"])
            .expect("parse cli");
        assert_eq!(cli.scan_mode, ScanMode::Parallel);
    }

    #[cfg(unix)]
    #[test]
    fn test_validation_fail_destination_nested_via_symlinked_parent_component() {
        use std::os::unix::fs::symlink;

        let src_dir = create_temp_dir();
        let alias_parent = create_temp_dir();
        let alias_path = alias_parent.path().join("alias");
        symlink(src_dir.path(), &alias_path).expect("create alias symlink");

        let config = Config {
            source: src_dir.path().to_path_buf(),
            destination: alias_path.join("nested"),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        if let Err(super::super::types::KopyError::Config(msg)) = result {
            assert!(msg.contains("cannot be nested"));
        } else {
            panic!("Expected Config error");
        }
    }
}
