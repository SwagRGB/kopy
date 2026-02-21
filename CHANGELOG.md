# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog.
Entries before `0.4.11` are backfilled from git history and version bump commits.

## [Unreleased]

### Added
- Parallel scanner implementation (`scan_directory_parallel`) using ignore crate parallel traversal with parity-focused filtering behavior.
- Scan mode selection controls via `--scan-mode` (`auto`, `sequential`, `parallel`).
- Auto scan-mode resolver with bounded probe heuristics for deep vs wide trees.
- `scan_bench` utility binary for local sequential/parallel scanner benchmarking and parity checks.
- Peak RSS reporting in `scan_bench` (Linux `VmHWM`) for memory profiling runs.
- Parallel transfer execution path via `execute_plan_parallel()` with size-aware routing:
  - small transfer actions run concurrently
  - large transfer actions run in a serialized lane to reduce I/O contention
- Single-file sync support (file source to destination file or destination directory), including checksum-safe diff behavior.
- Regression coverage for:
  - canonical-equal source/destination validation via symlink alias
  - file-vs-directory conflict planning paths
  - `DeleteMode::None` non-destructive delete behavior
  - parallel collector fallback trigger/no-trigger scenarios with sequential parity validation
  - high-cardinality small-transfer parallel execution completion path

### Fixed
- Reject source/destination roots that resolve to the same canonical directory (not only nested paths).
- Diff planning conflict handling for file-vs-directory path collisions.
- Permanent delete TOCTOU handling to treat post-check `NotFound` as success.
- Permanent delete error mapping to preserve typed `PermissionDenied`/`DiskFull` classifications.
- Single-file transfer reporting now reliably emits completion summary/progress reconciliation.
- Parallel executor no longer creates a nested Tokio runtime in library sync execution paths.

### Changed
- Reduced mutex lock scope in parallel scanning workers to avoid serializing metadata/filter work under contention.
- Sync scan path now routes through scan-mode resolution (manual override + auto default).
- Parallel scan progress callback delivery is serialized and monotonic while remaining live during traversal.
- Parallel scanner now switches from buffered collection to direct `FileTree` insertion when collector memory estimate exceeds threshold.
- Project runtime support narrowed to Linux-only execution paths (Windows behavior deferred).
- Parallel plan execution backend now uses panic-safe synchronous worker threads (runtime-agnostic for async embedders).
## [0.4.12] - 2026-02-18

### Added
- Additional API rustdoc coverage with runnable examples across public sync/scanner/diff/executor/config/type APIs.
- Linux tag-triggered release workflow that publishes:
  - `kopy-x86_64-unknown-linux-gnu.tar.gz`
  - matching `.sha256` checksum
- Release notes extraction from `CHANGELOG.md` based on tag version.
- MIT license file.

### Changed
- README rewritten in a user-facing format (overview, usage, configuration, limitations).
- Changelog backfilled with historical release entries from version bump history.

## [0.4.11] - 2026-02-12

### Added
- End-to-end integration coverage for sync command flows:
  - basic sync into empty destination
  - update/overwrite existing destination file
  - dry-run no-change verification
  - exclude pattern behavior
- Additional atomic copy unit coverage for basic content copy and parent directory creation.

### Changed
- README rewritten for end users (overview, usage, configuration, limitations).
- Public API documentation expanded with concise rustdoc examples.

## [0.4.8] - 2026-02-11

### Added
- Dry-run action listing.
- Progress/plan UI improvements:
  - human-readable plan preview
  - clearer transfer summary counts
  - grouped end-of-run error summaries
  - human-readable sync error messages

### Fixed
- Nested source/destination validation to prevent recursive sync growth.
- Symlink copy semantics and scanner behavior around hidden/include files.
- Skip-only plan short-circuiting and permanent delete race handling.

### Changed
- CI workflow reliability updates and lockfile tracking for `--locked` commands.

## [0.4.0] - 2026-02-07

### Added
- Atomic file copy with metadata preservation.
- Trash-based delete operations with snapshot manifests.
- Executor wiring for end-to-end sync execution.
- Initial scanning/transfer progress reporting.

## [0.3.0] - 2026-02-07

### Added
- Metadata diff engine and sync plan generation.
- Optional Blake3 checksum-based comparison.
- Transfer duration estimation helpers in plan statistics.

## [0.2.2] - 2026-02-06

### Changed
- Scanner readability and diagnostics improvements.
- Better symlink entry handling in scanner internals.

## [0.2.1] - 2026-02-06

### Added
- Recursive directory scanning with ignore-file support.
- Progress callback support during scan (files and bytes).
