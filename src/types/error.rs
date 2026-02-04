//! Error types for kopy

use std::path::PathBuf;
use thiserror::Error;

/// Error types
#[derive(Debug, Error)]
pub enum KopyError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Permission denied: {path}")]
    PermissionDenied { path: PathBuf },

    #[error("Disk full: {available} bytes available, {needed} bytes needed")]
    DiskFull { available: u64, needed: u64 },

    #[error("Checksum mismatch: {path}")]
    ChecksumMismatch { path: PathBuf },

    #[error("Transfer interrupted: {path} at offset {offset} bytes")]
    TransferInterrupted { path: PathBuf, offset: u64 },

    #[error("SSH connection failed: {0}")]
    SshError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}
