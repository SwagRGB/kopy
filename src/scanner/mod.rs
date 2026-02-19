//! Directory scanning logic

mod parallel;
mod walker;

pub use parallel::scan_directory_parallel;
pub use walker::{scan_directory, ProgressCallback};
