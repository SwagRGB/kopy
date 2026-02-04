//! Core type definitions for kopy

mod entry;
mod tree;
mod action;
mod error;

pub use entry::FileEntry;
pub use tree::FileTree;
pub use action::{SyncAction, DeleteMode};
pub use error::KopyError;
