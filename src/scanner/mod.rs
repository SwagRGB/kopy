//! Directory scanning logic

mod walker;

pub use walker::{scan_directory, ProgressCallback};
