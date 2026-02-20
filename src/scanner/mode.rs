use crate::config::{Config, ScanMode};
use crate::scanner::walker::{
    compile_patterns, is_destination_internal_trash, should_include_path,
};
use crate::types::KopyError;
use std::path::Path;
use std::time::{Duration, Instant};

const PROBE_ENTRY_LIMIT: usize = 512;
const PROBE_TIME_BUDGET: Duration = Duration::from_millis(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedScanMode {
    Sequential,
    Parallel,
}

#[derive(Default, Debug, Clone, Copy)]
struct ScanShape {
    // Entries seen by walker before include/exclude post-filtering.
    probed_entries: usize,
    // Entries retained after include/exclude filters.
    selected_entries: usize,
    sampled_files: usize,
    sampled_dirs: usize,
    max_depth: usize,
}

pub fn resolve_scan_mode(root_path: &Path, config: &Config) -> Result<ResolvedScanMode, KopyError> {
    match config.scan_mode {
        ScanMode::Sequential => Ok(ResolvedScanMode::Sequential),
        ScanMode::Parallel => Ok(ResolvedScanMode::Parallel),
        ScanMode::Auto => {
            if config.threads <= 1 {
                return Ok(ResolvedScanMode::Sequential);
            }
            let shape = sample_scan_shape(root_path, config)?;
            Ok(select_mode_from_shape(shape))
        }
    }
}

fn select_mode_from_shape(shape: ScanShape) -> ResolvedScanMode {
    // Tiny traversal probes often do better sequentially.
    if shape.probed_entries < 200 {
        return ResolvedScanMode::Sequential;
    }

    let deep_narrow = shape.max_depth >= 64
        && shape.sampled_files <= 1_200
        && shape.sampled_dirs > shape.sampled_files;
    if deep_narrow {
        return ResolvedScanMode::Sequential;
    }

    // If tree walk work is high but user filters aggressively, traversal still dominates.
    if shape.probed_entries >= 300 && shape.selected_entries < 120 {
        return ResolvedScanMode::Parallel;
    }

    ResolvedScanMode::Parallel
}

fn sample_scan_shape(root_path: &Path, config: &Config) -> Result<ScanShape, KopyError> {
    let exclude_patterns = compile_patterns(&config.exclude_patterns)?;
    let include_patterns = compile_patterns(&config.include_patterns)?;

    let walker = ignore::WalkBuilder::new(root_path)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .add_custom_ignore_filename(".kopyignore")
        .build();

    let mut shape = ScanShape::default();
    let start = Instant::now();

    for result in walker {
        if shape.probed_entries >= PROBE_ENTRY_LIMIT || start.elapsed() >= PROBE_TIME_BUDGET {
            break;
        }

        let entry = match result {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let file_type = match entry.file_type() {
            Some(ft) => ft,
            None => continue,
        };

        let relative_path = match entry.path().strip_prefix(root_path) {
            Ok(path) => path,
            Err(_) => continue,
        };

        if file_type.is_dir() || file_type.is_file() || file_type.is_symlink() {
            shape.probed_entries += 1;
        } else {
            continue;
        }

        let depth = relative_path.components().count();
        if depth > shape.max_depth {
            shape.max_depth = depth;
        }

        if !should_include_path(relative_path, &exclude_patterns, &include_patterns) {
            continue;
        }

        if is_destination_internal_trash(root_path, config, relative_path) {
            continue;
        }

        if file_type.is_dir() {
            shape.sampled_dirs += 1;
        } else if file_type.is_file() || file_type.is_symlink() {
            shape.sampled_files += 1;
        }
        shape.selected_entries += 1;
    }

    Ok(shape)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_select_mode_from_shape_prefers_sequential_for_small_samples() {
        let shape = ScanShape {
            probed_entries: 120,
            selected_entries: 100,
            sampled_files: 150,
            sampled_dirs: 50,
            max_depth: 4,
        };
        assert_eq!(select_mode_from_shape(shape), ResolvedScanMode::Sequential);
    }

    #[test]
    fn test_select_mode_from_shape_prefers_parallel_for_large_samples() {
        let shape = ScanShape {
            probed_entries: 512,
            selected_entries: 450,
            sampled_files: 1_500,
            sampled_dirs: 500,
            max_depth: 12,
        };
        assert_eq!(select_mode_from_shape(shape), ResolvedScanMode::Parallel);
    }

    #[test]
    fn test_select_mode_from_shape_prefers_sequential_for_deep_narrow_tree() {
        let shape = ScanShape {
            probed_entries: 420,
            selected_entries: 380,
            sampled_files: 400,
            sampled_dirs: 600,
            max_depth: 90,
        };
        assert_eq!(select_mode_from_shape(shape), ResolvedScanMode::Sequential);
    }

    #[test]
    fn test_select_mode_from_shape_uses_probe_load_not_filter_output() {
        let shape = ScanShape {
            probed_entries: 500,
            selected_entries: 10,
            sampled_files: 8,
            sampled_dirs: 2,
            max_depth: 8,
        };
        assert_eq!(select_mode_from_shape(shape), ResolvedScanMode::Parallel);
    }

    #[test]
    fn test_resolve_scan_mode_respects_manual_parallel() {
        let config = Config {
            scan_mode: ScanMode::Parallel,
            ..Config::default()
        };
        let mode = resolve_scan_mode(Path::new("."), &config).expect("resolve mode");
        assert_eq!(mode, ResolvedScanMode::Parallel);
    }

    #[test]
    fn test_resolve_scan_mode_auto_with_single_thread_prefers_sequential() {
        let config = Config {
            scan_mode: ScanMode::Auto,
            threads: 1,
            ..Config::default()
        };
        let mode = resolve_scan_mode(Path::new("."), &config).expect("resolve mode");
        assert_eq!(mode, ResolvedScanMode::Sequential);
    }
}
