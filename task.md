# kopy - Development Task Breakdown

> **Detailed checklist for implementing the modern file synchronization tool**

---

## Phase 1: MVP (4-6 weeks)

**Goal:** Working local sync with core safety features

### Week 1-2: Foundation & Setup

#### Project Initialization
- [x] Create Cargo project structure
- [x] Configure `Cargo.toml` with Phase 1 dependencies
- [x] Set up `.gitignore` and repository
- [x] Create module structure (lib.rs, main.rs)
- [x] Set up CI/CD pipeline (GitHub Actions)
  - [x] Rust fmt check
  - [x] Clippy lints
  - [x] Unit tests
  - [ ] Integration tests

#### Core Type System
- [ ] Implement `FileEntry` struct
  - [ ] Path field (relative)
  - [ ] Size, mtime, permissions
  - [ ] Optional hash field (lazy)
  - [ ] Symlink metadata
  - [ ] Serialization support (serde)
- [ ] Implement `FileTree` struct
  - [ ] HashMap storage (path → entry)
  - [ ] Aggregate statistics (total_size, total_files)
  - [ ] Helper methods (insert, get, contains)
- [ ] Implement `SyncAction` enum
  - [ ] CopyNew variant
  - [ ] Overwrite variant
  - [ ] Delete variant
  - [ ] Skip variant
- [ ] Implement `DeleteMode` enum
  - [ ] None, Trash, Permanent variants
- [ ] Implement `KopyError` enum
  - [ ] Use `thiserror` for derive macros
  - [ ] IO, PermissionDenied, DiskFull, etc.
  - [ ] Implement Display and Error traits

#### Configuration System
- [ ] Implement `Config` struct
  - [ ] Source and destination paths
  - [ ] Behavior flags (dry_run, checksum_mode, delete_mode)
  - [ ] Filtering (exclude, include patterns)
  - [ ] Performance settings (threads, bandwidth_limit)
- [ ] Implement config validation
  - [ ] Check paths exist
  - [ ] Validate exclude patterns
  - [ ] Ensure source != destination
- [ ] Add config defaults (Default trait)

#### CLI Argument Parsing
- [ ] Set up `clap` with derive macros
- [ ] Define main command arguments
  - [ ] Source path (required)
  - [ ] Destination path (required)
  - [ ] `--dry-run` flag
  - [ ] `--delete` flag
  - [ ] `--exclude` patterns (repeatable)
  - [ ] `--include` patterns (repeatable)
- [ ] Implement argument validation
- [ ] Add `--help` and `--version` support
- [ ] Create CLI → Config conversion

### Week 3-4: Core Sync Logic

#### Directory Scanner
- [ ] Implement sequential directory walker (walkdir)
- [ ] Create `scan_directory()` function
  - [ ] Walk directory tree recursively
  - [ ] Build FileEntry for each file
  - [ ] Populate FileTree
  - [ ] Handle symlinks correctly
  - [ ] Skip special files (pipes, sockets)
- [ ] Implement exclude pattern filtering
  - [ ] Use `globset` crate
  - [ ] Support `.gitignore` syntax
  - [ ] Load `.kopyignore` if present
  - [ ] Apply patterns during scan
- [ ] Add scan progress events
  - [ ] Emit file count updates
  - [ ] Emit size accumulation
- [ ] Error handling
  - [ ] Permission denied → skip with warning
  - [ ] Broken symlinks → skip with warning
  - [ ] Log all errors for summary

#### Diff Engine
- [ ] Implement `generate_sync_plan()` function
  - [ ] Compare source and dest trees
  - [ ] Generate Vec<SyncAction>
- [ ] Implement `compare_files()` function
  - [ ] Tier 1: Metadata comparison
    - [ ] Size mismatch → Overwrite
    - [ ] Source newer → Overwrite
    - [ ] Dest newer → Skip (Phase 1) / Conflict (Phase 2)
    - [ ] Identical → Skip
  - [ ] Tier 2: Content hash (if checksum_mode)
    - [ ] Lazy hash computation
    - [ ] Blake3 hashing
    - [ ] Cache hash in FileEntry
- [ ] Implement delete detection
  - [ ] Find files in dest but not in src
  - [ ] Generate Delete actions (if delete_mode != None)
- [ ] Add plan statistics
  - [ ] Count actions by type
  - [ ] Calculate total bytes to transfer
  - [ ] Estimate time remaining

#### File Operations
- [ ] Implement `copy_file_atomic()`
  - [ ] Create parent directories
  - [ ] Write to `.part` file
  - [ ] Stream copy with buffer (128KB)
  - [ ] Flush and sync to disk
  - [ ] Atomic rename to final path
  - [ ] Preserve metadata (permissions, mtime)
- [ ] Implement `move_to_trash()`
  - [ ] Create `.kopy_trash/TIMESTAMP/` directory
  - [ ] Calculate relative path
  - [ ] Atomic move to trash
  - [ ] Create MANIFEST.json
  - [ ] Log deletion metadata
- [ ] Add error handling
  - [ ] Disk full detection
  - [ ] Permission errors
  - [ ] Partial write recovery

#### Executor
- [ ] Implement `execute_plan()` (single-threaded)
  - [ ] Iterate through SyncActions
  - [ ] Execute each action sequentially
  - [ ] Emit progress events
- [ ] Implement action handlers
  - [ ] Handle CopyNew
  - [ ] Handle Overwrite
  - [ ] Handle Delete (trash mode)
  - [ ] Handle Skip (no-op)
- [ ] Add error recovery
  - [ ] Continue on non-fatal errors
  - [ ] Collect errors for summary
  - [ ] Rollback on critical errors

### Week 5-6: UX & Testing

#### Progress UI
- [ ] Implement basic progress bar (indicatif)
  - [ ] Scanning phase progress
  - [ ] Transfer phase progress
  - [ ] Current file indicator
  - [ ] Bytes/sec throughput
- [ ] Implement plan preview
  - [ ] Show action counts
  - [ ] Show total bytes
  - [ ] Format human-readable sizes
- [ ] Add dry-run output
  - [ ] List all planned actions
  - [ ] Show what would be deleted
  - [ ] No actual execution

#### Error Reporting
- [ ] Implement error summary
  - [ ] Collect all errors during sync
  - [ ] Display at end of run
  - [ ] Group by error type
- [ ] Add human-readable error messages
  - [ ] Convert IO errors to plain English
  - [ ] Suggest fixes for common issues
  - [ ] Include file paths in errors

#### Testing
- [ ] Unit tests for core modules
  - [ ] `scanner::` tests
    - [ ] Empty directory
    - [ ] Nested directories
    - [ ] Symlinks
    - [ ] Exclude patterns
  - [ ] `diff::` tests
    - [ ] New files
    - [ ] Modified files
    - [ ] Deleted files
    - [ ] Identical files
  - [ ] `executor::` tests
    - [ ] Atomic copy
    - [ ] Trash delete
    - [ ] Error handling
- [ ] Integration tests
  - [ ] Basic sync (empty dest)
  - [ ] Update existing files
  - [ ] Delete mode (trash)
  - [ ] Dry-run mode
  - [ ] Exclude patterns
- [ ] Test fixtures
  - [ ] Create sample directory structures
  - [ ] Generate test files
  - [ ] Use `tempfile` for cleanup

#### Documentation
- [ ] Write README.md
  - [ ] Installation instructions
  - [ ] Basic usage examples
  - [ ] Feature list
- [ ] Add inline code documentation
  - [ ] Document all public APIs
  - [ ] Add examples to doc comments
- [ ] Create CHANGELOG.md
- [ ] Add LICENSE file (MIT/Apache-2.0)

---

## Phase 2: Performance (3-4 weeks)

**Goal:** Make it fast and robust

### Week 7-8: Parallelization

#### Parallel Directory Scanning
- [ ] Integrate `jwalk` crate
- [ ] Implement `scan_directory_parallel()`
  - [ ] Use thread pool for traversal
  - [ ] Thread-safe FileTree (Arc<Mutex<>>)
  - [ ] Atomic counters for statistics
- [ ] Benchmark vs sequential walker
  - [ ] Test on 10,000 files
  - [ ] Test on deep directory trees
  - [ ] Measure speedup

#### Concurrent File Transfer
- [ ] Implement thread pool executor
  - [ ] Use `tokio` runtime
  - [ ] Create worker pool
  - [ ] MPSC channel for work distribution
- [ ] Implement `execute_plan_parallel()`
  - [ ] Partition plan by file size
  - [ ] Small files → parallel workers
  - [ ] Large files → single worker (avoid contention)
- [ ] Add concurrency limits
  - [ ] Max concurrent transfers
  - [ ] Configurable via `--threads` flag
- [ ] Handle race conditions
  - [ ] Directory creation (ensure parents exist)
  - [ ] Trash directory conflicts

#### Event-Driven Progress
- [ ] Implement `SyncEvent` enum
  - [ ] ScanStart, ScanProgress, ScanComplete
  - [ ] FileStart, Progress, FileComplete
  - [ ] Error, Conflict events
- [ ] Create event channel (mpsc)
  - [ ] Workers → Reporter
  - [ ] Non-blocking sends
- [ ] Implement `Reporter` struct
  - [ ] Subscribe to event channel
  - [ ] Update progress bars
  - [ ] Aggregate statistics

#### Multi-File Progress Display
- [ ] Use `indicatif::MultiProgress`
- [ ] Create multiple progress bars
  - [ ] Overall progress (total files)
  - [ ] Current file progress (bytes)
  - [ ] Throughput indicator
- [ ] Add real-time statistics
  - [ ] Files/sec transfer rate
  - [ ] Bytes/sec throughput
  - [ ] ETA calculation

#### Bandwidth Limiting
- [ ] Implement rate limiter
  - [ ] Token bucket algorithm
  - [ ] Configurable limit (bytes/sec)
- [ ] Integrate into copy loop
  - [ ] Sleep after each chunk
  - [ ] Calculate sleep duration
- [ ] Add `--limit` CLI flag
  - [ ] Parse human-readable sizes (5MB/s)
  - [ ] Validate input

### Week 9-10: Robustness

#### Blake3 Hashing
- [ ] Integrate `blake3` crate
- [ ] Implement `compute_hash()` function
  - [ ] Stream file in chunks
  - [ ] Incremental hashing
  - [ ] Return 32-byte hash
- [ ] Add hash caching
  - [ ] Store in FileEntry
  - [ ] Avoid recomputation
- [ ] Benchmark vs other algorithms
  - [ ] Compare to MD5, SHA256
  - [ ] Measure throughput

#### Checksum Mode
- [ ] Add `--checksum` flag
- [ ] Modify diff engine
  - [ ] Always compute hashes
  - [ ] Compare content, not just metadata
- [ ] Add checksum verification
  - [ ] Hash after copy
  - [ ] Compare to source hash
  - [ ] Retry on mismatch

#### Resume Capability
- [ ] Detect existing `.part` files
  - [ ] Check size vs expected
  - [ ] Validate integrity (optional hash)
- [ ] Implement resume logic
  - [ ] Seek to offset in source
  - [ ] Append to `.part` file
  - [ ] Continue from last position
- [ ] Add resume progress indicator
  - [ ] Show "Resuming from X%"
  - [ ] Update progress bar accordingly

#### Conflict Detection
- [ ] Implement conflict detection
  - [ ] Dest mtime > src mtime
  - [ ] Emit Conflict event
- [ ] Add interactive resolution
  - [ ] Prompt user for action
  - [ ] Options: Skip, Overwrite, Backup, Abort
  - [ ] Remember choice for batch operations
- [ ] Add `--conflict-strategy` flag
  - [ ] `skip`, `overwrite`, `backup`, `abort`
  - [ ] Non-interactive mode

#### Error Summary Report
- [ ] Collect all errors during sync
  - [ ] Store in Vec<KopyError>
  - [ ] Include file paths and context
- [ ] Display summary at end
  - [ ] Group by error type
  - [ ] Show file paths
  - [ ] Suggest remediation
- [ ] Add exit codes
  - [ ] 0 = success
  - [ ] 1 = partial success (some errors)
  - [ ] 2 = failure (critical error)

#### Comprehensive Error Messages
- [ ] Improve error formatting
  - [ ] Use `console` crate for colors
  - [ ] Highlight important information
  - [ ] Add context (what was being done)
- [ ] Add error suggestions
  - [ ] Permission denied → check ownership
  - [ ] Disk full → free up space
  - [ ] Network error → check connection

---

## Phase 3: Remote & Advanced (4-6 weeks)

**Goal:** SSH support and pro features

### Week 11-13: Remote Sync

#### SSH Client Integration
- [ ] Integrate `ssh2` crate
- [ ] Implement SSH connection
  - [ ] Parse user@host:path syntax
  - [ ] Support SSH key authentication
  - [ ] Support password authentication
  - [ ] Handle known_hosts verification
- [ ] Implement remote command execution
  - [ ] Execute `kopy --agent` on remote
  - [ ] Capture stdout/stderr
  - [ ] Handle exit codes

#### Agent Mode Protocol
- [ ] Implement `--agent` flag
  - [ ] Run in agent mode (no CLI)
  - [ ] Read commands from stdin
  - [ ] Write responses to stdout
- [ ] Define protocol messages
  - [ ] ScanRequest, ScanResponse
  - [ ] TransferRequest, TransferResponse
  - [ ] DeleteRequest, DeleteResponse
- [ ] Implement binary serialization
  - [ ] Use `bincode` for efficiency
  - [ ] Serialize FileTree
  - [ ] Serialize SyncActions

#### Remote Manifest Exchange
- [ ] Implement remote scan
  - [ ] Send ScanRequest to agent
  - [ ] Receive remote FileTree
  - [ ] Deserialize manifest
- [ ] Implement diff on local side
  - [ ] Compare local and remote trees
  - [ ] Generate sync plan
  - [ ] Send plan to agent
- [ ] Implement remote execution
  - [ ] Agent executes plan
  - [ ] Reports progress back
  - [ ] Handles errors

#### SFTP Fallback Mode
- [ ] Implement SFTP client
  - [ ] Use `ssh2::Sftp`
  - [ ] List remote directory
  - [ ] Transfer files via SFTP
- [ ] Add fallback logic
  - [ ] Try agent mode first
  - [ ] Fall back to SFTP if agent missing
  - [ ] Warn user about slower performance

#### Delta Transfer Algorithm
- [ ] Implement rolling checksum
  - [ ] Adler-32 weak checksum
  - [ ] Rolling window (4KB blocks)
- [ ] Implement signature generation
  - [ ] Split file into blocks
  - [ ] Compute weak + strong checksums
  - [ ] Return Vec<BlockSignature>
- [ ] Implement delta generation
  - [ ] Scan local file with rolling window
  - [ ] Find matching blocks
  - [ ] Generate delta instructions
- [ ] Implement delta application
  - [ ] Reconstruct file from delta
  - [ ] Verify integrity
- [ ] Add `--delta` flag
  - [ ] Enable delta transfer mode
  - [ ] Only for large files (>100MB)

### Week 14-16: Advanced Features

#### Watch Mode
- [ ] Integrate `notify` crate
- [ ] Implement filesystem watcher
  - [ ] Watch source directory recursively
  - [ ] Receive file events (create, modify, delete)
- [ ] Implement debouncing
  - [ ] Collect events for settle period
  - [ ] Trigger sync after quiet period
  - [ ] Configurable settle time (`--watch-settle`)
- [ ] Implement incremental sync
  - [ ] Build mini FileTree for changed files
  - [ ] Run diff engine on subset
  - [ ] Execute plan
- [ ] Add watch UI
  - [ ] Show "Watching..." status
  - [ ] Display detected changes
  - [ ] Show sync results
- [ ] Handle Ctrl+C gracefully
  - [ ] Clean shutdown
  - [ ] Finish in-progress transfers

#### Snapshot Backups
- [ ] Implement `--backup-dir` flag
  - [ ] Specify snapshot directory
  - [ ] Create timestamped subdirectories
- [ ] Implement backup logic
  - [ ] Before overwriting, copy old version to backup
  - [ ] Preserve directory structure
  - [ ] Add metadata (timestamp, reason)
- [ ] Add backup cleanup
  - [ ] `--keep-backups=N` flag
  - [ ] Delete old snapshots
  - [ ] Keep last N versions

#### Profile System
- [ ] Define profile format (TOML)
  - [ ] Source, destination paths
  - [ ] All config options
  - [ ] Named profiles
- [ ] Implement profile loading
  - [ ] Read from `~/.config/kopy/profiles.toml`
  - [ ] Parse TOML
  - [ ] Merge with CLI args (CLI overrides)
- [ ] Add `--profile` flag
  - [ ] Load named profile
  - [ ] Validate profile exists
- [ ] Add profile examples
  - [ ] Document in README
  - [ ] Provide sample profiles

#### Rename Detection
- [ ] Implement rename heuristic
  - [ ] Find deleted files in dest
  - [ ] Find new files in src
  - [ ] Match by size + mtime + hash
- [ ] Add `--detect-renames` flag
  - [ ] Opt-in feature (expensive)
  - [ ] Show detected renames
- [ ] Implement Move action
  - [ ] Rename instead of delete + copy
  - [ ] Faster and preserves metadata

#### Verify Subcommand
- [ ] Add `verify` subcommand
  - [ ] Non-destructive audit
  - [ ] Compare source and dest
  - [ ] Report differences
- [ ] Implement verification logic
  - [ ] Scan both trees
  - [ ] Compare all files
  - [ ] Hash all content
- [ ] Generate verification report
  - [ ] Matched files
  - [ ] Modified files (checksum mismatch)
  - [ ] Missing files
  - [ ] Extra files
- [ ] Add exit codes
  - [ ] 0 = perfect match
  - [ ] 1 = differences found

#### Trash Management Commands
- [ ] Add `trash list` subcommand
  - [ ] List all trash snapshots
  - [ ] Show file counts and sizes
  - [ ] Sort by timestamp
- [ ] Add `trash restore` subcommand
  - [ ] Restore specific file
  - [ ] Restore entire snapshot
  - [ ] Handle conflicts
- [ ] Add `trash clean` subcommand
  - [ ] `--older-than` flag (e.g., 30d)
  - [ ] `--all` flag (nuclear option)
  - [ ] Confirm before deletion
  - [ ] Show space freed

---

## Phase 4: Polish (2-3 weeks)

**Goal:** Production-ready UX

### Week 17-18: Quality

#### Comprehensive Error Messages
- [ ] Audit all error paths
  - [ ] Ensure all errors have context
  - [ ] Add suggestions for fixes
- [ ] Improve error formatting
  - [ ] Use colors and formatting
  - [ ] Show file paths clearly
  - [ ] Include error codes
- [ ] Add error documentation
  - [ ] Document common errors
  - [ ] Provide troubleshooting guide

#### Shell Completions
- [ ] Generate bash completions
  - [ ] Use `clap_complete`
  - [ ] Include all subcommands
  - [ ] Include all flags
- [ ] Generate zsh completions
- [ ] Generate fish completions
- [ ] Add installation instructions
  - [ ] Document where to place files
  - [ ] Provide install script

#### Man Page Generation
- [ ] Generate man page from clap
  - [ ] Use `clap_mangen`
  - [ ] Include all commands
  - [ ] Include examples
- [ ] Add detailed descriptions
  - [ ] Expand flag descriptions
  - [ ] Add usage examples
  - [ ] Include exit codes
- [ ] Install man page
  - [ ] Include in release builds
  - [ ] Document installation

#### Benchmarking Suite
- [ ] Create benchmark scenarios
  - [ ] 10,000 small files (1KB)
  - [ ] 100 large files (100MB)
  - [ ] Mixed workload
  - [ ] Deep directory trees
- [ ] Implement benchmarks
  - [ ] Use `criterion` crate
  - [ ] Measure time and throughput
  - [ ] Compare phases (1, 2, 3)
- [ ] Compare with rsync
  - [ ] Run same scenarios
  - [ ] Measure relative performance
  - [ ] Document results

#### Performance Profiling
- [ ] Profile CPU usage
  - [ ] Use `perf` or `flamegraph`
  - [ ] Identify hotspots
  - [ ] Optimize critical paths
- [ ] Profile memory usage
  - [ ] Use `valgrind` or `heaptrack`
  - [ ] Find memory leaks
  - [ ] Optimize allocations
- [ ] Profile I/O
  - [ ] Measure syscalls
  - [ ] Optimize buffer sizes
  - [ ] Reduce unnecessary I/O

#### Memory Optimization
- [ ] Audit memory usage
  - [ ] Test with large directories (1M+ files)
  - [ ] Ensure constant memory usage
- [ ] Optimize data structures
  - [ ] Use compact representations
  - [ ] Avoid unnecessary clones
  - [ ] Use references where possible
- [ ] Implement streaming
  - [ ] Don't load entire trees in memory
  - [ ] Stream file lists
  - [ ] Process incrementally

### Week 19: Release Prep

#### Documentation
- [ ] Write comprehensive README
  - [ ] Feature overview
  - [ ] Installation instructions
  - [ ] Usage examples
  - [ ] Comparison with rsync
- [ ] Create user guide
  - [ ] Getting started
  - [ ] Common workflows
  - [ ] Advanced features
  - [ ] Troubleshooting
- [ ] Write developer documentation
  - [ ] Architecture overview
  - [ ] Module descriptions
  - [ ] Contributing guidelines
- [ ] Add code examples
  - [ ] Example scripts
  - [ ] Integration examples

#### CI/CD Setup
- [ ] Configure GitHub Actions
  - [ ] Test on Linux, macOS, Windows
  - [ ] Multiple Rust versions
  - [ ] Code coverage reporting
- [ ] Add release workflow
  - [ ] Build release binaries
  - [ ] Cross-compilation
  - [ ] Upload to GitHub Releases
- [ ] Add dependency auditing
  - [ ] Security vulnerability scanning
  - [ ] License compliance check

#### Release Builds
- [ ] Configure release profile
  - [ ] Optimize for size and speed
  - [ ] Enable LTO
  - [ ] Strip debug symbols
- [ ] Cross-compile for targets
  - [ ] Linux (x86_64, aarch64)
  - [ ] macOS (x86_64, aarch64)
  - [ ] Windows (x86_64)
- [ ] Test release binaries
  - [ ] Run on each platform
  - [ ] Verify functionality
  - [ ] Check binary size

#### Changelog
- [ ] Document all changes
  - [ ] Group by category (features, fixes, breaking)
  - [ ] Include issue/PR references
  - [ ] Follow Keep a Changelog format
- [ ] Write release notes
  - [ ] Highlight major features
  - [ ] Migration guide (if breaking changes)
  - [ ] Known issues

#### 1.0 Announcement
- [ ] Write announcement post
  - [ ] Feature highlights
  - [ ] Comparison with rsync
  - [ ] Installation instructions
  - [ ] Call to action (try it, contribute)
- [ ] Publish to crates.io
  - [ ] Verify package contents
  - [ ] Test installation
  - [ ] Publish release
- [ ] Announce on social media
  - [ ] Reddit (r/rust)
  - [ ] Hacker News
  - [ ] Twitter/X
  - [ ] Rust community forums

---

## Ongoing Tasks

### Testing
- [ ] Maintain test coverage >80%
- [ ] Add tests for each new feature
- [ ] Run integration tests before each release
- [ ] Test on multiple platforms

### Documentation
- [ ] Keep README up to date
- [ ] Update CHANGELOG for each release
- [ ] Document new features
- [ ] Add examples for common use cases

### Performance
- [ ] Run benchmarks regularly
- [ ] Profile for regressions
- [ ] Optimize hot paths
- [ ] Monitor memory usage

### Community
- [ ] Respond to issues
- [ ] Review pull requests
- [ ] Help users with problems
- [ ] Gather feedback for improvements

---

## Success Metrics

### Phase 1
- [x] Successfully syncs 10,000 files without errors
- [x] Trash restore works 100% of the time
- [x] Dry-run accurately predicts actions
- [x] No data corruption on interrupt

### Phase 2
- [x] 2x faster than rsync for 10,000 small files
- [x] Resume works after interrupt at any point
- [x] Checksum mode catches all corruption
- [x] Bandwidth limiting within 5% of target

### Phase 3
- [x] SSH sync works with standard OpenSSH servers
- [x] Delta transfer reduces bandwidth by 80%+
- [x] Watch mode detects changes within 2 seconds
- [x] Profiles load and work correctly

### Phase 4
- [x] Zero critical bugs in issue tracker
- [x] Documentation complete and clear
- [x] 100+ GitHub stars
- [x] Positive feedback from beta testers

---

**Status:** 📋 Ready to begin implementation

**Next Action:** Initialize Cargo project and set up dependencies

**Target 1.0:** Q2 2026
