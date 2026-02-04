//! Execution coordinator

mod copy;
mod trash;

pub use copy::copy_file_atomic;
pub use trash::move_to_trash;

use crate::types::SyncAction;
use crate::Config;

/// Execute a sync plan
pub fn execute_plan(
    _plan: Vec<SyncAction>,
    _config: &Config,
) -> Result<(), crate::types::KopyError> {
    // TODO: Implement plan execution
    todo!("Implement execute_plan")
}
