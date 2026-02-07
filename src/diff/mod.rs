//! Diff engine - Comparison logic and plan generation

mod compare;
mod engine;
mod plan;

pub use compare::compare_files;
pub use engine::{DiffPlan, PlanStats};
pub use plan::generate_sync_plan;
