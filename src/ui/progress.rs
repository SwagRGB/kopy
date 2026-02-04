//! Progress reporting

/// Progress reporter for sync operations
pub struct ProgressReporter {
    // TODO: Add indicatif progress bars
}

impl ProgressReporter {
    /// Create a new progress reporter
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}
