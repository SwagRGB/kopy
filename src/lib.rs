//! # kopy - Modern File Synchronization Tool
//!
//! Safety by default, speed by design.
//!
//! A next-generation CLI synchronization tool that replaces `rsync` with
//! human-centric design, bulletproof safety, and zero-configuration operation.

// Module declarations
pub mod config;
pub mod scanner;
pub mod diff;
pub mod executor;
pub mod hash;
pub mod ui;
pub mod commands;
pub mod types;

// Re-export commonly used types
pub use types::{FileEntry, FileTree, SyncAction, DeleteMode, KopyError};
pub use config::Config;

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
