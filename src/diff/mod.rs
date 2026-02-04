//! Diff engine - Comparison logic and plan generation

mod compare;
mod plan;

pub use compare::compare_files;
pub use plan::generate_sync_plan;
