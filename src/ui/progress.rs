//! Progress reporting

use indicatif::{HumanBytes, ProgressBar, ProgressStyle};
use std::path::Path;
use std::time::Instant;

/// Progress reporter for sync operations
pub struct ProgressReporter {
    scan_bar: ProgressBar,
    transfer_bar: ProgressBar,
    transfer_started_at: Option<Instant>,
    transferred_bytes: u64,
}

impl ProgressReporter {
    /// Create a new progress reporter
    pub fn new() -> Self {
        let scan_bar = ProgressBar::new_spinner();
        scan_bar.enable_steady_tick(std::time::Duration::from_millis(120));
        if let Ok(style) = ProgressStyle::with_template("{spinner} {msg}") {
            scan_bar.set_style(style.tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "));
        }

        let transfer_bar = ProgressBar::new(0);
        if let Ok(style) =
            ProgressStyle::with_template("{bar:30.cyan/blue} {pos}/{len} files | {msg}")
        {
            transfer_bar.set_style(style.progress_chars("=>-"));
        }

        Self {
            scan_bar,
            transfer_bar,
            transfer_started_at: None,
            transferred_bytes: 0,
        }
    }

    /// Mark start of a scanning phase.
    pub fn start_scan(&self, label: &str) {
        self.scan_bar.set_message(format!("Scanning {}...", label));
    }

    /// Update scanning progress counters.
    pub fn update_scan(&self, label: &str, files: u64, bytes: u64) {
        self.scan_bar.set_message(format!(
            "Scanning {}... {} files | {}",
            label,
            files,
            HumanBytes(bytes)
        ));
    }

    /// Mark completion of a scanning phase.
    pub fn finish_scan(&self, label: &str, files: usize, bytes: u64) {
        self.scan_bar.finish_with_message(format!(
            "Scanned {}: {} files | {}",
            label,
            files,
            HumanBytes(bytes)
        ));
    }

    /// Initialize transfer phase progress.
    pub fn start_transfer(&mut self, total_transfer_files: u64) {
        self.transfer_started_at = Some(Instant::now());
        self.transferred_bytes = 0;
        self.transfer_bar.set_length(total_transfer_files);
        self.transfer_bar.set_position(0);
        self.transfer_bar
            .set_message("Starting transfer...".to_string());
    }

    /// Update current file/action indicator.
    pub fn set_current_file(&self, action: &str, path: Option<&Path>) {
        let path_display = path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        self.transfer_bar
            .set_message(format!("{} {}", action, path_display));
    }

    /// Mark one transfer file complete and refresh throughput display.
    pub fn complete_transfer_file(&mut self, bytes: u64) {
        self.transferred_bytes = self.transferred_bytes.saturating_add(bytes);
        self.transfer_bar.inc(1);

        let throughput = self.current_throughput_bps();
        self.transfer_bar.set_message(format!(
            "{} transferred | {}/s",
            HumanBytes(self.transferred_bytes),
            HumanBytes(throughput)
        ));
    }

    /// Surface an action error in transfer phase.
    pub fn transfer_error(&self, action: &str, path: Option<&Path>, err: &str) {
        let path_display = path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        self.transfer_bar
            .println(format!("ERROR {} {}: {}", action, path_display, err));
    }

    /// Finalize transfer phase.
    pub fn finish_transfer(
        &self,
        succeeded: usize,
        failed: usize,
        bytes: u64,
        transfers: usize,
        deletes: usize,
    ) {
        let throughput = self.current_throughput_bps();
        self.transfer_bar.finish_with_message(format!(
            "Actions complete: {} succeeded, {} failed | {} transfers, {} deletes | {} total | {}/s",
            succeeded,
            failed,
            transfers,
            deletes,
            HumanBytes(bytes),
            HumanBytes(throughput)
        ));
    }

    fn current_throughput_bps(&self) -> u64 {
        match self.transfer_started_at {
            Some(started) => {
                let elapsed = started.elapsed();
                let secs = elapsed.as_secs_f64();
                if secs > 0.0 {
                    (self.transferred_bytes as f64 / secs) as u64
                } else {
                    0
                }
            }
            None => 0,
        }
    }
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_transfer_progress_increments_position_and_bytes() {
        let mut reporter = ProgressReporter::new();
        reporter.start_transfer(2);

        reporter.complete_transfer_file(128);
        reporter.complete_transfer_file(256);

        assert_eq!(reporter.transfer_bar.position(), 2);
        assert_eq!(reporter.transfer_bar.length(), Some(2));
        assert_eq!(reporter.transferred_bytes, 384);
    }

    #[test]
    fn test_current_file_indicator_updates_message() {
        let reporter = ProgressReporter::new();
        reporter.set_current_file("Copy", Some(Path::new("a/b/file.txt")));

        let msg = reporter.transfer_bar.message();
        assert!(msg.contains("Copy"));
        assert!(msg.contains("a/b/file.txt"));
    }

    #[test]
    fn test_throughput_becomes_non_zero_after_transfer_time() {
        let mut reporter = ProgressReporter::new();
        reporter.start_transfer(1);
        thread::sleep(Duration::from_millis(30));
        reporter.complete_transfer_file(1024);

        assert!(reporter.current_throughput_bps() > 0);
    }

    #[test]
    fn test_scan_methods_execute_without_panicking() {
        let reporter = ProgressReporter::new();
        reporter.start_scan("source");
        reporter.update_scan("source", 3, 2048);
        reporter.finish_scan("source", 3, 2048);
    }
}
