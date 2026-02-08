//! Executor module for file operations

pub mod copy;
pub mod trash;

use crate::diff::DiffPlan;
use crate::types::KopyError;
use crate::Config;

// Re-export for convenience
pub use copy::copy_file_atomic;
pub use trash::move_to_trash;

/// Execute a sync plan
///
/// This will be implemented in Phase 1 after copy_file_atomic is complete
pub fn execute_plan(_plan: &DiffPlan, _config: &Config) -> Result<(), KopyError> {
    todo!("Implement execute_plan")
}
