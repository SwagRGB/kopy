//! Diff engine - Comparison logic and plan generation

mod compare;
mod plan;

pub use plan::generate_sync_plan;
pub use compare::compare_files;
