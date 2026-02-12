//! Error types for kopy

use std::path::PathBuf;
use thiserror::Error;

/// Error types for kopy operations
#[derive(Debug, Error)]
pub enum KopyError {
    /// Standard IO error (automatically converted via #[from])
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid configuration
    #[error("Configuration error: {0}")]
    Config(String),

    /// Validation error (logic checks)
    #[error("Validation error: {0}")]
    Validation(String),

    /// Permission denied for specific path
    #[error("Permission denied: {path}")]
    PermissionDenied { path: PathBuf },

    /// Disk full error
    #[error("Disk full: {available} bytes available, {needed} bytes needed")]
    DiskFull { available: u64, needed: u64 },

    /// Checksum mismatch detected
    #[error("Checksum mismatch: {path}")]
    ChecksumMismatch { path: PathBuf },

    /// Transfer was interrupted
    #[error("Transfer interrupted: {path} at offset {offset} bytes")]
    TransferInterrupted { path: PathBuf, offset: u64 },

    /// SSH connection error.
    #[error("SSH connection failed: {0}")]
    SshError(String),

    /// Dry run mode - safely abort execution
    #[error("Dry run mode: no changes were made")]
    DryRun,
}

impl KopyError {
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            KopyError::TransferInterrupted { .. } | KopyError::DryRun
        )
    }

    /// Check if this error is a validation error
    pub fn is_validation_error(&self) -> bool {
        matches!(self, KopyError::Validation(_) | KopyError::Config(_))
    }

    /// Check if this error is related to permissions
    pub fn is_permission_error(&self) -> bool {
        matches!(self, KopyError::PermissionDenied { .. })
    }

    /// Check if this error is related to disk space
    pub fn is_disk_space_error(&self) -> bool {
        matches!(self, KopyError::DiskFull { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error as IoError, ErrorKind};

    // Automatic Conversion Tests (#[from] macro)

    #[test]
    fn test_io_error_automatic_conversion() {
        // Test that std::io::Error automatically converts to KopyError::Io
        let io_error = IoError::new(ErrorKind::NotFound, "file not found");
        let kopy_error: KopyError = io_error.into();

        assert!(matches!(kopy_error, KopyError::Io(_)));
        assert!(kopy_error.to_string().contains("IO error"));
    }

    #[test]
    fn test_io_error_from_function() {
        // Test using ? operator with io::Error
        fn returns_io_error() -> Result<(), KopyError> {
            let _file = std::fs::File::open("/nonexistent/path/file.txt")?;
            Ok(())
        }

        let result = returns_io_error();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), KopyError::Io(_)));
    }

    // Variant Creation Tests

    #[test]
    fn test_config_error() {
        let error = KopyError::Config("Invalid source path".to_string());
        assert!(error.to_string().contains("Configuration error"));
        assert!(error.to_string().contains("Invalid source path"));
        assert!(error.is_validation_error());
    }

    #[test]
    fn test_validation_error() {
        let error = KopyError::Validation("Source path does not exist".to_string());
        assert!(error.to_string().contains("Validation error"));
        assert!(error.to_string().contains("Source path does not exist"));
        assert!(error.is_validation_error());
    }

    #[test]
    fn test_permission_denied() {
        let path = PathBuf::from("/protected/file.txt");
        let error = KopyError::PermissionDenied { path: path.clone() };
        assert!(error.to_string().contains("Permission denied"));
        assert!(error.to_string().contains("/protected/file.txt"));
        assert!(error.is_permission_error());
    }

    #[test]
    fn test_disk_full() {
        let error = KopyError::DiskFull {
            available: 1024,
            needed: 2048,
        };
        assert!(error.to_string().contains("Disk full"));
        assert!(error.to_string().contains("1024"));
        assert!(error.to_string().contains("2048"));
        assert!(error.is_disk_space_error());
    }

    #[test]
    fn test_checksum_mismatch() {
        let path = PathBuf::from("corrupted.dat");
        let error = KopyError::ChecksumMismatch { path };
        assert!(error.to_string().contains("Checksum mismatch"));
        assert!(error.to_string().contains("corrupted.dat"));
    }

    #[test]
    fn test_transfer_interrupted() {
        let path = PathBuf::from("large_file.bin");
        let error = KopyError::TransferInterrupted {
            path: path.clone(),
            offset: 1048576,
        };
        assert!(error.to_string().contains("Transfer interrupted"));
        assert!(error.to_string().contains("large_file.bin"));
        assert!(error.to_string().contains("1048576"));
        assert!(error.is_recoverable());
    }

    #[test]
    fn test_ssh_error() {
        let error = KopyError::SshError("Connection timeout".to_string());
        assert!(error.to_string().contains("SSH connection failed"));
        assert!(error.to_string().contains("Connection timeout"));
    }

    #[test]
    fn test_dry_run() {
        let error = KopyError::DryRun;
        assert!(error.to_string().contains("Dry run mode"));
        assert!(error.to_string().contains("no changes were made"));
        assert!(error.is_recoverable());
    }

    // Helper Method Tests

    #[test]
    fn test_is_recoverable() {
        assert!(KopyError::TransferInterrupted {
            path: PathBuf::from("file.txt"),
            offset: 100
        }
        .is_recoverable());
        assert!(KopyError::DryRun.is_recoverable());

        assert!(!KopyError::Config("error".to_string()).is_recoverable());
        assert!(!KopyError::DiskFull {
            available: 0,
            needed: 100
        }
        .is_recoverable());
    }

    #[test]
    fn test_is_validation_error() {
        assert!(KopyError::Config("error".to_string()).is_validation_error());
        assert!(KopyError::Validation("error".to_string()).is_validation_error());

        assert!(!KopyError::DryRun.is_validation_error());
        assert!(!KopyError::Io(IoError::new(ErrorKind::NotFound, "test")).is_validation_error());
    }

    #[test]
    fn test_is_permission_error() {
        assert!(KopyError::PermissionDenied {
            path: PathBuf::from("file.txt")
        }
        .is_permission_error());

        assert!(!KopyError::Config("error".to_string()).is_permission_error());
        assert!(!KopyError::DryRun.is_permission_error());
    }

    #[test]
    fn test_is_disk_space_error() {
        assert!(KopyError::DiskFull {
            available: 0,
            needed: 100
        }
        .is_disk_space_error());

        assert!(!KopyError::Config("error".to_string()).is_disk_space_error());
        assert!(!KopyError::DryRun.is_disk_space_error());
    }

    // Error Trait Tests

    #[test]
    fn test_error_trait_implementation() {
        use std::error::Error;

        let error = KopyError::Config("test".to_string());
        let _error_ref: &dyn Error = &error;

        // Verify Display is implemented (via to_string)
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn test_debug_implementation() {
        let error = KopyError::Validation("test".to_string());
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("Validation"));
    }

    // Result Type Usage Tests

    #[test]
    fn test_result_propagation() {
        fn inner_function() -> Result<(), KopyError> {
            Err(KopyError::Config("test error".to_string()))
        }

        fn outer_function() -> Result<(), KopyError> {
            inner_function()?;
            Ok(())
        }

        let result = outer_function();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), KopyError::Config(_)));
    }
}
