//! Core type definitions for kopy

mod action;
mod entry;
mod error;
mod tree;

pub use action::{DeleteMode, SyncAction};
pub use entry::FileEntry;
pub use error::KopyError;
pub use tree::FileTree;
