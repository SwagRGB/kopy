//! Configuration management

use super::types::DeleteMode;
use clap::{Parser, ValueEnum};
use std::fs;
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

    /// Transfer worker threads (0 = auto based on CPU cores).
    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// Storage profile used to tune transfer classification defaults.
    #[arg(long, value_enum, default_value_t = StorageProfile::Auto)]
    pub storage_profile: StorageProfile,

    /// Threshold that classifies transfer actions as large (e.g. 16MiB, 64MB).
    #[arg(long, value_parser = parse_human_size)]
    pub large_transfer_threshold: Option<u64>,
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

/// Storage tuning profile for transfer scheduling heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StorageProfile {
    Auto,
    Hdd,
    Ssd,
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

    /// Storage profile used for performance defaults.
    pub storage_profile: StorageProfile,

    /// Threshold for classifying large transfer actions.
    pub large_transfer_threshold_bytes: u64,

    /// True when `storage_profile=auto` fell back to SSD due unknown device type.
    pub storage_profile_auto_fallback: bool,

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
            threads: 0,
            scan_mode: ScanMode::Auto,
            storage_profile: StorageProfile::Auto,
            large_transfer_threshold_bytes: DEFAULT_SSD_TRANSFER_THRESHOLD_BYTES,
            storage_profile_auto_fallback: false,
            bandwidth_limit: None,
            backup_dir: None,
            watch: false,
            watch_settle: 2,
        }
    }
}

impl Config {
    /// Resolve configured worker threads into an execution count.
    ///
    /// `0` means auto, which derives from host CPU parallelism and is clamped
    /// to a safe upper bound to avoid accidental oversubscription.
    pub fn resolved_threads(&self) -> usize {
        resolve_threads(self.threads)
    }

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

const MAX_AUTO_THREADS: usize = 64;
// Baseline defaults from mixed-size transfer benchmarks; can be overridden via CLI.
const DEFAULT_HDD_TRANSFER_THRESHOLD_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_SSD_TRANSFER_THRESHOLD_BYTES: u64 = 32 * 1024 * 1024;

fn resolve_threads(requested: usize) -> usize {
    if requested == 0 {
        let detected = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
        return detected.clamp(1, MAX_AUTO_THREADS);
    }

    requested.clamp(1, MAX_AUTO_THREADS)
}

fn resolve_large_transfer_threshold(
    explicit_threshold: Option<u64>,
    storage_profile: StorageProfile,
) -> u64 {
    if let Some(value) = explicit_threshold {
        return value;
    }

    match storage_profile {
        StorageProfile::Hdd => DEFAULT_HDD_TRANSFER_THRESHOLD_BYTES,
        StorageProfile::Ssd | StorageProfile::Auto => DEFAULT_SSD_TRANSFER_THRESHOLD_BYTES,
    }
}

fn resolve_effective_storage_profile(
    requested_profile: StorageProfile,
    source: &Path,
    destination: &Path,
) -> (StorageProfile, bool) {
    if requested_profile != StorageProfile::Auto {
        return (requested_profile, false);
    }

    let source_rotational = detect_rotational_storage_for_path(source);
    let destination_rotational = detect_rotational_storage_for_path(destination);
    if source_rotational == Some(true) || destination_rotational == Some(true) {
        return (StorageProfile::Hdd, false);
    }
    if source_rotational == Some(false) && destination_rotational == Some(false) {
        return (StorageProfile::Ssd, false);
    }

    (StorageProfile::Ssd, true)
}

#[cfg(target_os = "linux")]
fn detect_rotational_storage_for_path(path: &Path) -> Option<bool> {
    let existing_path = nearest_existing_path(path)?;
    let canonical_path = existing_path.canonicalize().ok().unwrap_or(existing_path);
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let major_minor = select_mount_major_minor(&canonical_path, &mountinfo)?;
    read_rotational_for_major_minor(&major_minor)
}

#[cfg(not(target_os = "linux"))]
fn detect_rotational_storage_for_path(_path: &Path) -> Option<bool> {
    None
}

fn nearest_existing_path(path: &Path) -> Option<PathBuf> {
    let mut cursor = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };

    loop {
        if cursor.exists() {
            return Some(cursor);
        }
        if !cursor.pop() {
            return None;
        }
    }
}

#[cfg(target_os = "linux")]
fn read_rotational_for_major_minor(major_minor: &str) -> Option<bool> {
    let direct = PathBuf::from(format!("/sys/dev/block/{major_minor}/queue/rotational"));
    if let Some(value) = read_rotational_file(&direct) {
        return Some(value);
    }

    // If MAJ:MIN points at a partition, walk ancestors to find parent disk queue settings.
    let mut cursor = fs::canonicalize(format!("/sys/dev/block/{major_minor}")).ok()?;
    loop {
        let candidate = cursor.join("queue/rotational");
        if let Some(value) = read_rotational_file(&candidate) {
            return Some(value);
        }
        if !cursor.pop() {
            break;
        }
    }

    None
}

#[cfg(not(target_os = "linux"))]
fn read_rotational_for_major_minor(_major_minor: &str) -> Option<bool> {
    None
}

fn read_rotational_file(path: &Path) -> Option<bool> {
    let value = fs::read_to_string(path).ok()?;
    match value.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn select_mount_major_minor(path: &Path, mountinfo: &str) -> Option<String> {
    let mut best: Option<(usize, String)> = None;

    for line in mountinfo.lines() {
        let Some((left, _)) = line.split_once(" - ") else {
            continue;
        };
        let fields: Vec<&str> = left.split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }

        let major_minor = fields[2];
        let mount_point = decode_mountinfo_field(fields[4]);
        let mount_path = Path::new(&mount_point);
        if !path.starts_with(mount_path) {
            continue;
        }

        let depth = mount_path.components().count();
        match &best {
            Some((best_depth, _)) if *best_depth >= depth => {}
            _ => best = Some((depth, major_minor.to_string())),
        }
    }

    best.map(|(_, major_minor)| major_minor)
}

fn decode_mountinfo_field(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let bytes = field.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
        {
            let octal = &field[i + 1..i + 4];
            if let Ok(value) = u8::from_str_radix(octal, 8) {
                out.push(char::from(value));
                i += 4;
                continue;
            }
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

fn parse_human_size(value: &str) -> Result<u64, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("size cannot be empty".to_string());
    }

    let split_idx = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (digits, suffix) = trimmed.split_at(split_idx);
    if digits.is_empty() {
        return Err(format!("invalid size '{}': missing numeric prefix", value));
    }

    let base = digits
        .parse::<u64>()
        .map_err(|_| format!("invalid size '{}': numeric value is too large", value))?;
    if base == 0 {
        return Err("size must be greater than 0".to_string());
    }

    let unit = suffix.trim().to_ascii_lowercase();
    let multiplier: u64 = match unit.as_str() {
        "" | "b" => 1,
        "k" | "kb" => 1_000,
        "m" | "mb" => 1_000_000,
        "g" | "gb" => 1_000_000_000,
        "kib" => 1_024,
        "mib" => 1_048_576,
        "gib" => 1_073_741_824,
        _ => {
            return Err(format!(
                "invalid size unit '{}': use B, KB, MB, GB, KiB, MiB, or GiB",
                suffix.trim()
            ));
        }
    };

    base.checked_mul(multiplier)
        .ok_or_else(|| format!("invalid size '{}': value overflows u64", value))
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
        let (effective_storage_profile, auto_fallback) =
            resolve_effective_storage_profile(cli.storage_profile, &cli.source, &cli.destination);

        let config = Config {
            source: cli.source,
            destination: cli.destination,
            dry_run: cli.dry_run,
            checksum_mode: cli.checksum,
            delete_mode,
            exclude_patterns: cli.exclude,
            include_patterns: cli.include,
            scan_mode: cli.scan_mode,
            threads: cli.threads,
            storage_profile: effective_storage_profile,
            large_transfer_threshold_bytes: resolve_large_transfer_threshold(
                cli.large_transfer_threshold,
                effective_storage_profile,
            ),
            storage_profile_auto_fallback: auto_fallback,
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
        assert_eq!(config.threads, 0);
        assert_eq!(config.storage_profile, StorageProfile::Auto);
        assert_eq!(
            config.large_transfer_threshold_bytes,
            DEFAULT_SSD_TRANSFER_THRESHOLD_BYTES
        );
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
            threads: 7,
            storage_profile: StorageProfile::Ssd,
            large_transfer_threshold: None,
        };

        let config = Config::try_from(cli).unwrap();

        assert_eq!(config.delete_mode, DeleteMode::Trash);
        assert_eq!(config.exclude_patterns, vec!["*.tmp"]);
        assert_eq!(config.include_patterns, vec!["*.rs"]);
        assert_eq!(config.scan_mode, ScanMode::Auto);
        assert_eq!(config.threads, 7);
        assert_eq!(config.storage_profile, StorageProfile::Ssd);
        assert_eq!(
            config.large_transfer_threshold_bytes,
            DEFAULT_SSD_TRANSFER_THRESHOLD_BYTES
        );
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
            threads: 0,
            storage_profile: StorageProfile::Auto,
            large_transfer_threshold: None,
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
            threads: 0,
            storage_profile: StorageProfile::Auto,
            large_transfer_threshold: None,
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
            threads: 0,
            storage_profile: StorageProfile::Auto,
            large_transfer_threshold: None,
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
            threads: 0,
            storage_profile: StorageProfile::Auto,
            large_transfer_threshold: None,
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

    #[test]
    fn test_cli_parse_threads_explicit() {
        let cli = Cli::try_parse_from(["kopy", "src", "dst", "--threads", "8"]).expect("parse cli");
        assert_eq!(cli.threads, 8);
    }

    #[test]
    fn test_cli_parse_threads_default_auto() {
        let cli = Cli::try_parse_from(["kopy", "src", "dst"]).expect("parse cli");
        assert_eq!(cli.threads, 0);
    }

    #[test]
    fn test_cli_parse_storage_profile_hdd() {
        let cli = Cli::try_parse_from(["kopy", "src", "dst", "--storage-profile", "hdd"])
            .expect("parse cli");
        assert_eq!(cli.storage_profile, StorageProfile::Hdd);
    }

    #[test]
    fn test_cli_parse_large_transfer_threshold_human_size() {
        let cli =
            Cli::try_parse_from(["kopy", "src", "dst", "--large-transfer-threshold", "64MiB"])
                .expect("parse cli");
        assert_eq!(cli.large_transfer_threshold, Some(64 * 1024 * 1024));
    }

    #[test]
    fn test_cli_parse_large_transfer_threshold_rejects_zero() {
        let result = Cli::try_parse_from(["kopy", "src", "dst", "--large-transfer-threshold", "0"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolved_threads_auto_uses_host_parallelism() {
        let config = Config {
            threads: 0,
            ..Default::default()
        };
        let resolved = config.resolved_threads();
        assert!(resolved >= 1);
        assert!(resolved <= MAX_AUTO_THREADS);
    }

    #[test]
    fn test_resolved_threads_clamps_upper_bound() {
        let config = Config {
            threads: usize::MAX,
            ..Default::default()
        };
        assert_eq!(config.resolved_threads(), MAX_AUTO_THREADS);
    }

    #[test]
    fn test_config_uses_profile_default_threshold_hdd() {
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
            threads: 0,
            storage_profile: StorageProfile::Hdd,
            large_transfer_threshold: None,
        };
        let config = Config::try_from(cli).expect("convert config");
        assert_eq!(
            config.large_transfer_threshold_bytes,
            DEFAULT_HDD_TRANSFER_THRESHOLD_BYTES
        );
    }

    #[test]
    fn test_config_explicit_threshold_overrides_profile() {
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
            threads: 0,
            storage_profile: StorageProfile::Hdd,
            large_transfer_threshold: Some(128 * 1024 * 1024),
        };
        let config = Config::try_from(cli).expect("convert config");
        assert_eq!(config.large_transfer_threshold_bytes, 128 * 1024 * 1024);
    }

    #[test]
    fn test_decode_mountinfo_field_unescapes_spaces() {
        assert_eq!(
            decode_mountinfo_field("/mnt/with\\040space"),
            "/mnt/with space"
        );
    }

    #[test]
    fn test_select_mount_major_minor_prefers_longest_mount_prefix() {
        let mountinfo = "\
20 1 8:1 / / rw,relatime - ext4 /dev/sda1 rw\n\
31 20 8:17 / /mnt/storage rw,relatime - ext4 /dev/sdb1 rw\n\
42 31 8:18 / /mnt/storage/test-chamber rw,relatime - ext4 /dev/sdb2 rw\n";
        let selected =
            select_mount_major_minor(Path::new("/mnt/storage/test-chamber/data"), mountinfo);
        assert_eq!(selected, Some("8:18".to_string()));
    }

    #[test]
    fn test_auto_profile_falls_back_to_ssd_when_detection_unavailable() {
        let resolved = resolve_effective_storage_profile(
            StorageProfile::Auto,
            Path::new("/nonexistent/source/path"),
            Path::new("/another/nonexistent/destination/path"),
        );
        assert_eq!(resolved.0, StorageProfile::Ssd);
    }

    #[test]
    fn test_explicit_profile_never_sets_auto_fallback() {
        let resolved = resolve_effective_storage_profile(
            StorageProfile::Hdd,
            Path::new("/tmp"),
            Path::new("/tmp"),
        );
        assert_eq!(resolved, (StorageProfile::Hdd, false));
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
