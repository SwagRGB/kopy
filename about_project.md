# kopy - Modern File Synchronization Tool

> **"Safety by default, speed by design."**

A next-generation CLI synchronization tool that replaces `rsync` with human-centric design, bulletproof safety, and zero-configuration operation.

---

## Why kopy?

### The Problem with rsync

Despite being the gold standard for 30+ years, `rsync` has critical UX issues:

- **Deletes are permanent and terrifying**: `--delete` has no undo button. One typo = data loss.
- **Cryptic flags**: What's the difference between `-avzP` and `-rltDvu --delete-after`? Who knows without the man page.
- **Silent failures**: Files fail to transfer, no summary report, you only find out weeks later.
- **No progress for many small files**: Progress bar freezes on 10,000 tiny files while transfer continues.
- **Hostile error messages**: `rsync: send_files failed to open "/path/file": Permission denied (13)` - now what?

### The kopy Solution

```bash
# Simple, safe, obvious
kopy src/ dest/

# With deletes? They go to trash first
kopy src/ dest/ --delete

# Changed your mind? Undo it
kopy restore dest/

# Watch and auto-sync
kopy src/ dest/ --watch

# Verify backups (non-destructive)
kopy verify src/ dest/
```

**Core Values:**
- **Safety**: Trash-can deletes, atomic operations, dry-run previews
- **Clarity**: Plain English output, real-time progress, error summaries
- **Speed**: Parallel transfers, smart hashing, resumable operations
- **Zero Config**: Works out of the box, profiles for advanced use

---

## Feature Set

### Core Features (Phase 1 - MVP)

**Local Synchronization**
- Recursive directory sync with deep tree support
- Metadata-based diff (size + mtime) with optional content checksums
- Atomic operations via `.part` files (no corruption on interrupt)
- Trash-can deletes instead of permanent removal
- Exclude patterns (`.gitignore`, `.kopyignore`, `--exclude`)
- Single-threaded execution with clear progress indicators

**Safety Guarantees**
```bash
# Default: Non-destructive (won't delete anything)
kopy photos/ backup/photos/

# Delete mode: Moves to trash instead of unlinking
kopy photos/ backup/photos/ --delete
# → Deleted files go to backup/photos/.kopy_trash/2026-02-04_143022/

# Permanent delete (only after reviewing trash)
kopy cleanup backup/photos/ --older-than 30d
```

**Smart Progress Display**
```
Scanning directories... ━━━━━━━━━━━━━━━━━━━━ 100%

Plan:
  Copy:      1,247 files (4.2 GB)
  Update:      512 files (890 MB) 
  Delete:       89 files → trash
  Skip:      8,901 files (unchanged)

Proceed? [Y/n]

Copying... ━━━━━━━╸░░░░░░░░░░░ 45% | 562/1,247 files | 1.2 GB/s
  Current: IMG_2041.jpg (12.4 MB/s)
```

### Advanced Features (Phase 2)

**Concurrency & Performance**
- Thread pool for parallel small file transfers
- Chunked transfers for large files (>100MB)
- Resume capability (detects `.part` files and continues from offset)
- Blake3 hashing with SIMD acceleration

**Intelligent Diffing**
```rust
// Cascading verification algorithm
For each file pair (F_src, F_dest):
  1. Tier 1: Metadata Check (instant)
     - Size mismatch? → Modified
     - Source newer? → Modified  
     - Dest newer? → Conflict (warn user)
     - Identical? → Skip or goto Tier 2
  
  2. Tier 2: Content Hash (on-demand)
     - Only if --checksum flag OR metadata identical but suspicious
     - Blake3 hash both files
     - Mismatch? → Modified
  
  3. Tier 3: Delta Transfer (large files only)
     - For files >100MB marked as modified
     - Use rolling checksum (see Remote Features)
```

**Conflict Handling**
```bash
# Destination file is newer than source
⚠ Conflict detected: dest/report.pdf
  Source: 2026-02-01 14:30 (2.1 MB)
  Dest:   2026-02-04 09:15 (2.3 MB) ← newer

  Options:
  [s] Skip (keep dest)
  [o] Overwrite (use source) 
  [b] Backup (move dest to trash, copy source)
  [a] Abort sync
  
Choice [s/o/b/a]:
```

### Professional Features (Phase 3)

**Remote Sync (SSH)**

Instead of a custom protocol, kopy uses SSH tunneling with an agent model:

```bash
# Automatic agent deployment
kopy src/ user@server:/backup/

# How it works:
1. SSH to server
2. Check if 'kopy' binary exists remotely
3. If yes → Spawn 'kopy --agent' mode
   - Both sides compute hashes locally
   - Send compact file lists (bincode serialized)
   - Transfer only diffs
4. If no → Fall back to SFTP mode (slower but works)
```

**Agent Mode Protocol:**
```
[Local kopy] ←→ SSH ←→ [Remote kopy --agent]
      ↓                         ↓
   Scan src/                Scan dest/
   Hash files               Hash files
      ↓                         ↓
   Build manifest          Build manifest
      ↓————— Send via SSH ————→↓
   Diff engine calculates transfer plan
   Transfer only changed blocks
```

**Delta Transfers (rsync-style rolling checksum)**

For large modified files, avoid sending the entire file:

```
Given: F_src (local), F_dest (remote, modified)

1. Remote: Split F_dest into 4KB blocks
2. Remote: Compute signatures
   - Weak: Adler-32 (rolling checksum)
   - Strong: Blake3 hash per block
3. Remote → Local: Send signatures (small, ~50 bytes/block)
4. Local: Scan F_src with rolling window
   - Find matching blocks
   - Mark non-matching regions as "delta"
5. Local → Remote: Send deltas + reconstruction recipe
6. Remote: Rebuild F_dest using old blocks + new data
```

**Watch Mode**
```bash
kopy src/ dest/ --watch

# Uses filesystem events (notify crate)
Watching src/ for changes...
  14:32:15 │ Modified: src/main.rs
  14:32:15 │ Modified: src/utils.rs
  14:32:17 │ Debouncing... (2s settle time)
  14:32:19 │ Syncing 2 changed files...
  14:32:20 │ ✓ Sync complete
```

Features:
- Debouncing (ignores rapid-fire changes like `git checkout`)
- Configurable settle time (`--watch-settle=5s`)
- Graceful handling of platform differences

**Snapshot Backups**
```bash
# Time-machine style versioning
kopy src/ dest/ --backup-dir dest/.snapshots/

# Result:
dest/
  file.txt (current)
  .snapshots/
    2026-02-04_143000/
      file.txt (old version #1)
    2026-02-03_090000/  
      file.txt (old version #2)
```

**Profiles**
```toml
# ~/.config/kopy/profiles.toml
[my-server]
source = "/home/user/docs"
destination = "user@server:/backup/docs"
delete = true
exclude = ["*.tmp", "node_modules/", ".git/"]
bandwidth_limit = "10MB/s"

[photo-backup]
source = "/home/user/Photos" 
destination = "/mnt/nas/photos"
checksum = true  # Paranoid mode
backup_dir = "/mnt/nas/photos/.history"
```

```bash
# Use profile
kopy --profile my-server

# Override profile settings
kopy --profile my-server --dry-run
```

### Utility Commands

**Verify (Non-Destructive Audit)**
```bash
kopy verify src/ dest/

# Output:
Verification Report:
  ✓ Matched:     8,901 files
  ⚠ Modified:      512 files (checksum mismatch)
  ✗ Missing:        47 files (in src, not in dest)
  ⊕ Extra:          12 files (in dest, not in src)

Modified files:
  src/data.json
    Expected: blake3:a3f2e1...
    Got:      blake3:7b9c4d...
  
Missing files:
  src/report.pdf
  src/images/logo.png
```

**Trash Management**
```bash
# List trashed files
kopy trash list dest/

# Restore specific file
kopy trash restore dest/.kopy_trash/2026-02-04_143022/file.txt

# Restore entire trash snapshot
kopy trash restore dest/.kopy_trash/2026-02-04_143022/

# Cleanup old trash (permanent delete)
kopy trash clean dest/ --older-than 30d
kopy trash clean dest/ --all  # Nuclear option
```

---

## Architecture

### Pipeline Design (Producer-Consumer Pattern)

```
┌─────────────┐
│  Discovery  │ (Producer)
│   Scanner   │
└──────┬──────┘
       │ FileEntry trees
       ↓
┌─────────────┐
│ Diff Engine │ (Logic)
│  Comparator │
└──────┬──────┘
       │ SyncAction plan
       ↓
┌─────────────┐
│  Executor   │ (Consumer)
│ Thread Pool │
└──────┬──────┘
       │ Events
       ↓
┌─────────────┐
│  Reporter   │ (UI)
│ Progress UI │
└─────────────┘
```

### Stage Details

**1. Discovery (Parallel Scanning)**
```rust
// Scan both trees concurrently
let (src_tree, dest_tree) = tokio::join!(
    scan_directory(src_path),   // Uses jwalk (parallel walker)
    scan_directory(dest_path)
);

struct FileTree {
    entries: HashMap<PathBuf, FileEntry>,
    total_size: u64,
    total_files: usize,
}
```

**2. Diff Engine**
```rust
fn generate_plan(src: &FileTree, dest: &FileTree, config: &Config) -> Vec<SyncAction> {
    let mut plan = Vec::new();
    
    // Files in source
    for (path, src_entry) in &src.entries {
        match dest.entries.get(path) {
            None => plan.push(SyncAction::CopyNew(src_entry.clone())),
            Some(dest_entry) => {
                if needs_update(src_entry, dest_entry, config) {
                    plan.push(SyncAction::Overwrite(src_entry.clone()));
                }
            }
        }
    }
    
    // Files only in destination
    if config.delete_mode != DeleteMode::None {
        for path in dest.entries.keys() {
            if !src.entries.contains_key(path) {
                plan.push(SyncAction::Delete(path.clone()));
            }
        }
    }
    
    plan
}
```

**3. Executor (Concurrent Transfer)**
```rust
async fn execute_plan(plan: Vec<SyncAction>, config: &Config) {
    let (tx, rx) = mpsc::channel(100);
    
    // Worker pool
    for _ in 0..config.threads {
        let rx = rx.clone();
        tokio::spawn(async move {
            while let Some(action) = rx.recv().await {
                execute_action(action).await;
            }
        });
    }
    
    // Feed work to pool
    for action in plan {
        tx.send(action).await;
    }
}

async fn execute_action(action: SyncAction) {
    match action {
        SyncAction::CopyNew(entry) | SyncAction::Overwrite(entry) => {
            copy_with_atomic(&entry).await; // Uses .part file
        }
        SyncAction::Delete(path) => {
            move_to_trash(&path).await;
        }
        SyncAction::Skip => {}
    }
}
```

**4. Reporter (Real-time UI)**
```rust
use indicatif::{MultiProgress, ProgressBar};

struct Reporter {
    multi: MultiProgress,
    overall: ProgressBar,
    current_file: ProgressBar,
}

impl Reporter {
    fn update(&self, event: SyncEvent) {
        match event {
            SyncEvent::FileStart(name, size) => {
                self.current_file.set_message(name);
                self.current_file.set_length(size);
            }
            SyncEvent::Progress(bytes) => {
                self.current_file.inc(bytes);
                self.overall.inc(bytes);
            }
            SyncEvent::FileComplete => {
                self.overall.inc(1);
            }
        }
    }
}
```

---

## Technical Stack

### Dependencies (Rust)

```toml
[dependencies]
# CLI & Config
clap = { version = "4.5", features = ["derive", "color"] }
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"

# Async Runtime
tokio = { version = "1.40", features = ["full"] }

# File System
walkdir = "2.5"           # Simple walker (Phase 1)
jwalk = "0.8"             # Parallel walker (Phase 2)
camino = "1.1"            # UTF-8 paths
notify = "6.1"            # Filesystem events (watch mode)

# Hashing & Crypto
blake3 = "1.5"            # Fast, parallel hashing

# Compression & Serialization
bincode = "1.3"           # Binary serialization for network

# Pattern Matching
globset = "0.4"           # For .gitignore style patterns

# Progress & UI
indicatif = "0.17"        # Progress bars
console = "0.15"          # Terminal colors & formatting

# SSH (Phase 3)
ssh2 = "0.9"              # SSH client
```

### Core Data Structures

```rust
use std::path::PathBuf;
use std::time::SystemTime;
use serde::{Serialize, Deserialize};

/// Represents a file in the sync tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative path from root
    pub path: PathBuf,
    
    /// File size in bytes
    pub size: u64,
    
    /// Last modification time
    pub mtime: SystemTime,
    
    /// Unix permissions (mode)
    pub permissions: u32,
    
    /// Content hash (computed lazily)
    pub hash: Option<[u8; 32]>,
    
    /// Is this a symlink?
    pub is_symlink: bool,
    
    /// Symlink target (if applicable)
    pub symlink_target: Option<PathBuf>,
}

/// Sync action determined by diff engine
#[derive(Debug, Clone)]
pub enum SyncAction {
    /// Copy new file (exists in src, missing in dest)
    CopyNew(FileEntry),
    
    /// Overwrite existing file (src and dest differ)
    Overwrite(FileEntry),
    
    /// Delete file (exists in dest, missing in src)
    Delete(PathBuf),
    
    /// Move/rename detection (optional optimization)
    Move { from: PathBuf, to: PathBuf },
    
    /// Skip (files identical)
    Skip,
}

/// Delete behavior
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeleteMode {
    /// Don't delete anything
    None,
    
    /// Move to .kopy_trash/
    Trash,
    
    /// Permanent deletion (dangerous)
    Permanent,
}

/// Global configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Source directory
    pub source: PathBuf,
    
    /// Destination directory
    pub destination: PathBuf,
    
    /// Dry run (show plan, don't execute)
    pub dry_run: bool,
    
    /// Force checksum verification (slow but paranoid)
    pub checksum_mode: bool,
    
    /// How to handle deletes
    pub delete_mode: DeleteMode,
    
    /// Exclude patterns (globs)
    pub exclude: Vec<String>,
    
    /// Include patterns (overrides excludes)
    pub include: Vec<String>,
    
    /// Number of worker threads
    pub threads: usize,
    
    /// Bandwidth limit (bytes/sec, None = unlimited)
    pub bandwidth_limit: Option<u64>,
    
    /// Backup directory for snapshots
    pub backup_dir: Option<PathBuf>,
    
    /// Watch mode enabled?
    pub watch: bool,
    
    /// Watch settle time (seconds)
    pub watch_settle: u64,
}

/// Events emitted during sync
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Scan started
    ScanStart { path: PathBuf },
    
    /// Scan progress
    ScanProgress { files: usize, size: u64 },
    
    /// Scan complete
    ScanComplete { tree: FileTree },
    
    /// File transfer started
    FileStart { path: PathBuf, size: u64 },
    
    /// Transfer progress
    Progress { bytes: u64 },
    
    /// File complete
    FileComplete { path: PathBuf },
    
    /// Error occurred
    Error { path: PathBuf, error: String },
    
    /// Conflict detected
    Conflict { path: PathBuf, reason: String },
}

/// Error types
#[derive(Debug, thiserror::Error)]
pub enum KopyError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Permission denied: {path}")]
    PermissionDenied { path: PathBuf },
    
    #[error("Destination full: {available} bytes available, {needed} bytes needed")]
    DiskFull { available: u64, needed: u64 },
    
    #[error("Checksum mismatch: {path}")]
    ChecksumMismatch { path: PathBuf },
    
    #[error("Transfer interrupted: {path} at {offset} bytes")]
    TransferInterrupted { path: PathBuf, offset: u64 },
}
```

---

## Development Phases

### Phase 1: MVP (4-6 weeks)
**Goal:** Working local sync with core safety features

- [ ] CLI argument parsing (clap)
- [ ] Directory scanner (walkdir)
- [ ] Basic diff engine (metadata only)
- [ ] Single-threaded file copy with `.part` files
- [ ] Trash-can delete implementation
- [ ] Simple progress bar (single file)
- [ ] Exclude patterns (.gitignore parsing)
- [ ] Dry-run mode
- [ ] Basic error handling

**Deliverable:** `kopy src/ dest/` works reliably for local directories

### Phase 2: Performance (3-4 weeks)
**Goal:** Make it fast and robust

- [ ] Parallel directory walking (jwalk)
- [ ] Thread pool for concurrent transfers
- [ ] Blake3 hashing implementation
- [ ] Checksum mode (`--checksum`)
- [ ] Resume capability (detect and continue `.part` files)
- [ ] Multi-file progress display (indicatif multi-bar)
- [ ] Conflict detection (dest newer than src)
- [ ] Error summary report
- [ ] Bandwidth limiting

**Deliverable:** Fast, resumable syncs with clear progress

### Phase 3: Remote & Advanced (4-6 weeks)
**Goal:** SSH support and pro features

- [ ] SSH client integration (ssh2)
- [ ] Agent mode protocol
- [ ] Delta transfer algorithm (rolling checksum)
- [ ] Watch mode (notify + debouncing)
- [ ] Snapshot backups (`--backup-dir`)
- [ ] Profile system (config file loading)
- [ ] Rename detection heuristic
- [ ] Verify subcommand

**Deliverable:** Full-featured rsync replacement

### Phase 4: Polish (2-3 weeks)
**Goal:** Production-ready UX

- [ ] Comprehensive error messages
- [ ] Interactive conflict resolution
- [ ] Trash management commands
- [ ] Shell completions (bash, zsh, fish)
- [ ] Man page generation
- [ ] Benchmarking suite
- [ ] Integration tests
- [ ] Documentation

**Deliverable:** 1.0 release

---

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_diff_engine_new_file() {
        let mut src = FileTree::new();
        src.insert(FileEntry { path: "new.txt", ... });
        
        let dest = FileTree::new();
        
        let plan = generate_plan(&src, &dest, &Config::default());
        
        assert_eq!(plan.len(), 1);
        assert!(matches!(plan[0], SyncAction::CopyNew(_)));
    }
    
    #[test]
    fn test_trash_delete() {
        let temp = TempDir::new()?;
        let file = temp.path().join("test.txt");
        fs::write(&file, "data")?;
        
        move_to_trash(&file, &temp.path())?;
        
        assert!(!file.exists());
        assert!(temp.path().join(".kopy_trash").exists());
    }
}
```

### Integration Tests
```rust
#[test]
fn test_basic_sync() {
    let src = TempDir::new()?;
    let dest = TempDir::new()?;
    
    fs::write(src.path().join("file.txt"), "hello")?;
    
    let config = Config {
        source: src.path().to_path_buf(),
        destination: dest.path().to_path_buf(),
        ..Default::default()
    };
    
    sync(config)?;
    
    assert_eq!(
        fs::read_to_string(dest.path().join("file.txt"))?,
        "hello"
    );
}
```

### Benchmarks
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_blake3_vs_md5(c: &mut Criterion) {
    let data = vec![0u8; 1024 * 1024]; // 1MB
    
    c.bench_function("blake3_1mb", |b| {
        b.iter(|| blake3::hash(black_box(&data)))
    });
    
    c.bench_function("md5_1mb", |b| {
        b.iter(|| md5::compute(black_box(&data)))
    });
}

criterion_group!(benches, bench_blake3_vs_md5);
criterion_main!(benches);
```

---

## Usage Examples

### Basic Usage
```bash
# Simple copy
kopy photos/ /mnt/backup/photos/

# With deletes (safe mode)
kopy photos/ /mnt/backup/photos/ --delete

# Dry run first
kopy photos/ /mnt/backup/photos/ --delete --dry-run

# Paranoid mode (verify every byte)
kopy photos/ /mnt/backup/photos/ --checksum
```

### Advanced Usage
```bash
# Exclude patterns
kopy src/ dest/ --exclude "*.tmp" --exclude "node_modules/"

# Include overrides exclude
kopy src/ dest/ --exclude "*.log" --include "important.log"

# Bandwidth limit
kopy large-files/ dest/ --limit 5MB/s

# Snapshot backup
kopy docs/ /backup/docs/ --backup-dir /backup/docs/.history

# Watch and auto-sync
kopy ~/code/project/ /mnt/dev-backup/project/ --watch

# Remote sync
kopy ~/photos/ user@nas:/backup/photos/

# Use profile
kopy --profile daily-backup
```

### Maintenance
```bash
# Verify backup integrity
kopy verify photos/ /mnt/backup/photos/

# List trash
kopy trash list /mnt/backup/photos/

# Restore deleted file
kopy trash restore /mnt/backup/photos/.kopy_trash/2026-02-04_143022/IMG_2041.jpg

# Cleanup old trash
kopy trash clean /mnt/backup/photos/ --older-than 30d
```

---

## Inspiration & Prior Art

### What We Learn From:
- **rsync**: Delta transfer algorithm, proven reliability
- **rclone**: Cloud sync patterns, config profiles  
- **restic**: Snapshot model, content-addressed storage
- **dura**: Git-based backups, commit history UX
- **watchexec**: Filesystem watching with debouncing
- **exa/eza**: Modern CLI UX (colors, icons, clarity)

### Our Differentiation:
1. **Safety-first design**: Trash over delete, atomic ops, dry-run defaults
2. **Zero-config simplicity**: Works immediately, profiles optional
3. **Human-readable output**: No cryptic flags or error codes
4. **Local-optimized**: Fast LAN transfers before cloud sync
5. **Undo-friendly**: Trash, snapshots, verify commands

---

## Open Questions & Future Work

### Rename Detection Algorithm
Current idea: Content-based heuristic
```rust
// If file deleted in dest, check if any new file in src has:
// 1. Same size
// 2. Same mtime (within tolerance)
// 3. Same hash (if already computed)
// → Suggest this is a rename

// Issues:
// - False positives (duplicate files)
// - Performance cost of extra hashing
// - Ambiguity (multiple candidates)

// Solution: Make opt-in via --detect-renames flag
```

### Symbolic Link Handling Edge Cases
- Circular symlinks (infinite loops)
- Broken symlinks (point to non-existent targets)
- Cross-device links
- Absolute vs relative link preservation

### Future Features (Post 1.0)
- **Compression**: On-the-fly gzip/zstd for remote transfers
- **Encryption**: End-to-end encrypted backups
- **Deduplication**: Content-addressed storage for space savings
- **Cloud backends**: S3, GCS, Azure Blob direct sync
- **GUI**: Optional Electron/Tauri interface for non-CLI users
- **Scheduling**: Built-in cron-like task runner
- **Notifications**: Desktop/email alerts on completion/errors

---

## Contributing Guidelines

### Code Style
- Use `rustfmt` and `clippy` (enforced in CI)
- Prefer explicit error types over `.unwrap()`
- Document all public APIs with `///` comments
- Add tests for every new feature

### Commit Messages
```
feat: Add delta transfer for large files
fix: Handle permission errors gracefully  
docs: Update README with new examples
test: Add integration tests for watch mode
```

### PR Process
1. Fork and create feature branch
2. Write tests (coverage >80%)
3. Update documentation
4. Pass CI (tests, clippy, fmt)
5. Request review

---

## License

MIT or Apache 2.0 (dual license, standard for Rust projects)

---

## Acknowledgments

Inspired by decades of rsync reliability, modernized for 2026 workflows.

Built with Rust 🦀 — for performance, safety, and joy.

---

**Status:** 🚧 Specification complete, implementation in progress

**Start Date:** February 2026

**Target 1.0:** Q2 2026

---

## Quick Start for Developers

```bash
# Clone repo (when exists)
git clone https://github.com/yourusername/kopy
cd kopy

# Build
cargo build --release

# Run tests
cargo test

# Install locally
cargo install --path .

# Try it
kopy --help
```

Now go vibe code! 🚀
