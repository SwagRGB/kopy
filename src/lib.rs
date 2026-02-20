//! # kopy - Modern File Synchronization Tool
//!
//! Safety by default, speed by design.
//!
//! A next-generation CLI synchronization tool that replaces `rsync` with
//! human-centric design, bulletproof safety, and zero-configuration operation.

pub mod commands;
pub mod config;
pub mod diff;
pub mod executor;
pub mod hash;
pub mod scanner;
pub mod types;
pub mod ui;

pub use config::{Cli, Config, ScanMode};
pub use types::{DeleteMode, FileEntry, FileTree, KopyError, SyncAction};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
