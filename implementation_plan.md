# kopy - Implementation Plan

> **Modern File Synchronization Tool - Technical Implementation Guide**

---

## Executive Summary

This document outlines the technical implementation strategy for **kopy**, a next-generation file synchronization tool designed to replace `rsync` with improved safety, clarity, and user experience. The implementation follows a phased approach spanning 4 development phases over approximately 13-19 weeks.

**Core Principles:**
- Safety by default (trash-based deletes, atomic operations)
- Human-centric UX (clear progress, plain English errors)
- Performance without complexity (parallel operations, smart hashing)
- Zero-configuration operation (works immediately, profiles optional)

---

## Architecture Overview

### System Design Pattern

```
┌─────────────────────────────────────────────────────────────┐
│                      CLI Entry Point                         │
│              (Argument Parsing & Validation)                 │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ↓
┌─────────────────────────────────────────────────────────────┐
│                   Configuration Layer                        │
│        (Profiles, Excludes, Flags → Config Struct)          │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ↓
┌─────────────────────────────────────────────────────────────┐
│                    Discovery Stage                           │
│         (Parallel Directory Scanning - Producer)             │
│                                                              │
│  ┌──────────────┐              ┌──────────────┐            │
│  │ Source Tree  │              │  Dest Tree   │            │
│  │   Scanner    │              │   Scanner    │            │
│  └──────┬───────┘              └──────┬───────┘            │
│         │                              │                    │
│         └──────────────┬───────────────┘                    │
│                        ↓                                     │
│              ┌──────────────────┐                           │
│              │  FileTree Maps   │                           │
│              └──────────────────┘                           │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ↓
┌─────────────────────────────────────────────────────────────┐
│                     Diff Engine                              │
│           (Comparison Logic & Plan Generation)               │
│                                                              │
│  Input: (src_tree, dest_tree, config)                       │
│  Output: Vec<SyncAction>                                     │
│                                                              │
│  Algorithms:                                                 │
│  • Metadata comparison (size, mtime)                        │
│  • Content hashing (Blake3, lazy evaluation)                │
│  • Conflict detection (dest newer than src)                 │
│  • Delete planning (trash vs permanent)                     │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ↓
┌─────────────────────────────────────────────────────────────┐
│                   Execution Stage                            │
│          (Concurrent Transfer - Consumer Pool)               │
│                                                              │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐           │
│  │  Worker 1  │  │  Worker 2  │  │  Worker N  │           │
│  └─────┬──────┘  └─────┬──────┘  └─────┬──────┘           │
│        │                │                │                   │
│        └────────────────┴────────────────┘                   │
│                         │                                     │
│                    Event Stream                              │
│                         │                                     │
└─────────────────────────┼────────────────────────────────────┘
                          │
                          ↓
┌─────────────────────────────────────────────────────────────┐
│                    Reporter/UI Layer                         │
│              (Progress Bars, Status Updates)                 │
│                                                              │
│  • Multi-progress bars (overall + current file)             │
│  • Real-time throughput calculation                         │
│  • Error aggregation and summary                            │
└─────────────────────────────────────────────────────────────┘
```

---

## Core Algorithms & Pseudocode

### Algorithm 1: Directory Scanning (Discovery)

**Purpose:** Build complete file tree representations of source and destination directories.

**Complexity:** O(n) where n = total files
**Parallelization:** Yes (Phase 2 - jwalk)

```rust
// Pseudocode for directory scanning
FUNCTION scan_directory(root_path: Path) -> FileTree:
    tree = new FileTree()
    
    // Phase 1: Sequential walker
    FOR EACH entry IN walkdir(root_path):
        IF entry.is_file():
            relative_path = entry.path().strip_prefix(root_path)
            
            file_entry = FileEntry {
                path: relative_path,
                size: entry.metadata().len(),
                mtime: entry.metadata().modified(),
                permissions: entry.metadata().permissions(),
                hash: None,  // Lazy evaluation
                is_symlink: entry.is_symlink(),
                symlink_target: IF is_symlink THEN readlink(entry.path())
            }
            
            tree.insert(relative_path, file_entry)
            tree.total_size += file_entry.size
            tree.total_files += 1
            
            // Emit progress event
            emit(ScanProgress { files: tree.total_files, size: tree.total_size })
        
        ELSE IF entry.is_dir():
            // Recurse (handled by walkdir)
            CONTINUE
        
        // Apply exclude patterns
        IF matches_exclude_pattern(entry.path(), config.exclude):
            CONTINUE
    
    RETURN tree
```

**Optimization (Phase 2 - Parallel Walker):**

```rust
FUNCTION scan_directory_parallel(root_path: Path) -> FileTree:
    // Use jwalk for parallel directory traversal
    tree = new ConcurrentFileTree()  // Thread-safe HashMap
    
    jwalk(root_path)
        .parallelism(ThreadPoolBuilder::new().num_threads(num_cpus))
        .for_each(|entry| {
            // Same logic as above, but concurrent
            // Uses atomic counters for total_size/total_files
            tree.insert_concurrent(relative_path, file_entry)
        })
    
    RETURN tree.into_regular_tree()
```

---

### Algorithm 2: Diff Engine (Comparison Logic)

**Purpose:** Determine what actions are needed to synchronize source to destination.

**Complexity:** O(n + m) where n = src files, m = dest files
**Strategy:** Cascading verification (metadata → hash → delta)

```rust
FUNCTION generate_sync_plan(
    src_tree: FileTree,
    dest_tree: FileTree,
    config: Config
) -> Vec<SyncAction>:
    
    plan = []
    
    // ═══════════════════════════════════════════════════════
    // PHASE 1: Process files in source
    // ═══════════════════════════════════════════════════════
    FOR EACH (path, src_entry) IN src_tree.entries:
        
        MATCH dest_tree.get(path):
            
            // ───────────────────────────────────────────────
            // Case 1: File doesn't exist in destination
            // ───────────────────────────────────────────────
            None:
                plan.push(SyncAction::CopyNew(src_entry))
            
            // ───────────────────────────────────────────────
            // Case 2: File exists in both locations
            // ───────────────────────────────────────────────
            Some(dest_entry):
                action = compare_files(src_entry, dest_entry, config)
                IF action != SyncAction::Skip:
                    plan.push(action)
    
    // ═══════════════════════════════════════════════════════
    // PHASE 2: Process files only in destination (deletes)
    // ═══════════════════════════════════════════════════════
    IF config.delete_mode != DeleteMode::None:
        FOR EACH (path, dest_entry) IN dest_tree.entries:
            IF NOT src_tree.contains(path):
                plan.push(SyncAction::Delete(path))
    
    RETURN plan


// ═══════════════════════════════════════════════════════════
// Cascading File Comparison Algorithm
// ═══════════════════════════════════════════════════════════
FUNCTION compare_files(
    src: FileEntry,
    dest: FileEntry,
    config: Config
) -> SyncAction:
    
    // ───────────────────────────────────────────────────────
    // TIER 1: Metadata-based comparison (instant, no I/O)
    // ───────────────────────────────────────────────────────
    
    // Size mismatch = definitely different
    IF src.size != dest.size:
        RETURN SyncAction::Overwrite(src)
    
    // Modification time comparison
    time_diff = src.mtime - dest.mtime
    
    IF time_diff > 0:
        // Source is newer → update needed
        RETURN SyncAction::Overwrite(src)
    
    ELSE IF time_diff < 0:
        // Destination is newer → CONFLICT!
        RETURN SyncAction::Conflict {
            path: src.path,
            reason: "Destination file is newer than source",
            src_mtime: src.mtime,
            dest_mtime: dest.mtime
        }
    
    // ───────────────────────────────────────────────────────
    // TIER 2: Content-based comparison (expensive, on-demand)
    // ───────────────────────────────────────────────────────
    
    // Only hash if:
    // 1. User explicitly requested --checksum mode, OR
    // 2. Metadata identical but file is "suspicious"
    //    (e.g., same size/mtime but different inode)
    
    IF config.checksum_mode OR is_suspicious(src, dest):
        src_hash = compute_hash_lazy(src)   // Cache result
        dest_hash = compute_hash_lazy(dest)
        
        IF src_hash != dest_hash:
            RETURN SyncAction::Overwrite(src)
    
    // Files are identical
    RETURN SyncAction::Skip


// ───────────────────────────────────────────────────────────
// Lazy Hash Computation (with caching)
// ───────────────────────────────────────────────────────────
FUNCTION compute_hash_lazy(entry: FileEntry) -> [u8; 32]:
    // Check if already computed
    IF entry.hash.is_some():
        RETURN entry.hash.unwrap()
    
    // Compute Blake3 hash
    file = open(entry.path)
    hasher = Blake3::new()
    
    // Stream file in chunks (memory efficient)
    buffer = [0u8; 64 * 1024]  // 64KB chunks
    LOOP:
        bytes_read = file.read(buffer)
        IF bytes_read == 0:
            BREAK
        hasher.update(buffer[0..bytes_read])
    
    hash = hasher.finalize()
    entry.hash = Some(hash)  // Cache for future use
    
    RETURN hash
```

---

### Algorithm 3: Atomic File Copy (with Resume Support)

**Purpose:** Copy files safely with corruption protection and resume capability.

**Strategy:** 
1. Write to temporary `.part` file
2. Verify integrity (optional hash check)
3. Atomic rename to final destination

```rust
FUNCTION copy_file_atomic(
    src_path: Path,
    dest_path: Path,
    expected_size: u64
) -> Result<()>:
    
    part_path = dest_path.with_extension(".part")
    
    // ═══════════════════════════════════════════════════════
    // RESUME DETECTION: Check for existing .part file
    // ═══════════════════════════════════════════════════════
    start_offset = 0
    
    IF part_path.exists():
        part_size = part_path.metadata().len()
        
        // Validate partial file isn't corrupted
        IF part_size < expected_size:
            // Resume from this offset
            start_offset = part_size
            emit(ResumeDetected { path: dest_path, offset: start_offset })
        ELSE:
            // Partial file is larger than expected? Corruption!
            delete(part_path)
            start_offset = 0
    
    // ═══════════════════════════════════════════════════════
    // TRANSFER: Stream copy with progress reporting
    // ═══════════════════════════════════════════════════════
    src_file = open(src_path, READ_ONLY)
    dest_file = open(part_path, CREATE | APPEND)
    
    // Seek to resume position
    IF start_offset > 0:
        src_file.seek(start_offset)
        dest_file.seek(start_offset)
    
    buffer = [0u8; 128 * 1024]  // 128KB buffer
    total_copied = start_offset
    
    LOOP:
        bytes_read = src_file.read(buffer)
        IF bytes_read == 0:
            BREAK
        
        dest_file.write_all(buffer[0..bytes_read])
        total_copied += bytes_read
        
        // Emit progress event
        emit(Progress { bytes: bytes_read })
        
        // Bandwidth limiting (if configured)
        IF config.bandwidth_limit.is_some():
            sleep_for_rate_limit(bytes_read, config.bandwidth_limit)
    
    // ═══════════════════════════════════════════════════════
    // VERIFICATION: Ensure complete transfer
    // ═══════════════════════════════════════════════════════
    dest_file.flush()
    dest_file.sync_all()  // Force OS to write to disk
    
    final_size = part_path.metadata().len()
    IF final_size != expected_size:
        RETURN Error(TransferInterrupted {
            path: dest_path,
            offset: final_size
        })
    
    // ═══════════════════════════════════════════════════════
    // ATOMIC COMMIT: Rename .part to final name
    // ═══════════════════════════════════════════════════════
    // This is atomic on POSIX systems (single syscall)
    rename(part_path, dest_path)
    
    // ═══════════════════════════════════════════════════════
    // METADATA PRESERVATION: Copy permissions and timestamps
    // ═══════════════════════════════════════════════════════
    src_metadata = src_path.metadata()
    set_permissions(dest_path, src_metadata.permissions())
    set_times(dest_path, src_metadata.modified())
    
    RETURN Ok(())
```

---

### Algorithm 4: Trash-Based Delete (Undo-Friendly)

**Purpose:** Move files to timestamped trash directory instead of permanent deletion.

**Benefits:**
- User can review deleted files before permanent removal
- Accidental deletes are recoverable
- Trash can be cleaned up with `--older-than` flag

```rust
FUNCTION move_to_trash(
    file_path: Path,
    dest_root: Path
) -> Result<()>:
    
    // ═══════════════════════════════════════════════════════
    // TRASH DIRECTORY STRUCTURE:
    // dest_root/.kopy_trash/YYYY-MM-DD_HHMMSS/relative/path
    // ═══════════════════════════════════════════════════════
    
    timestamp = now().format("%Y-%m-%d_%H%M%S")
    trash_root = dest_root.join(".kopy_trash").join(timestamp)
    
    // Calculate relative path from dest_root
    relative_path = file_path.strip_prefix(dest_root)
    trash_path = trash_root.join(relative_path)
    
    // Create parent directories
    create_dir_all(trash_path.parent())
    
    // ═══════════════════════════════════════════════════════
    // ATOMIC MOVE: Single rename operation (fast)
    // ═══════════════════════════════════════════════════════
    // Note: rename() is atomic within same filesystem
    // If cross-filesystem, falls back to copy + delete
    
    TRY:
        rename(file_path, trash_path)
    CATCH CrossDeviceError:
        // Fallback: copy then delete
        copy(file_path, trash_path)
        delete(file_path)
    
    // ═══════════════════════════════════════════════════════
    // METADATA LOGGING: Record deletion for audit trail
    // ═══════════════════════════════════════════════════════
    manifest_path = trash_root.join("MANIFEST.json")
    manifest = load_or_create(manifest_path)
    
    manifest.deleted_files.push({
        original_path: relative_path,
        trash_path: trash_path,
        deleted_at: timestamp,
        size: file_path.metadata().len(),
        reason: "sync_delete"
    })
    
    save(manifest_path, manifest)
    
    RETURN Ok(())


// ═══════════════════════════════════════════════════════════
// Trash Restoration Algorithm
// ═══════════════════════════════════════════════════════════
FUNCTION restore_from_trash(
    trash_snapshot: Path,  // e.g., .kopy_trash/2026-02-04_143022/
    dest_root: Path
) -> Result<()>:
    
    manifest = load(trash_snapshot.join("MANIFEST.json"))
    
    FOR EACH entry IN manifest.deleted_files:
        trash_file = trash_snapshot.join(entry.original_path)
        restore_path = dest_root.join(entry.original_path)
        
        // Check for conflicts
        IF restore_path.exists():
            prompt_user("File exists at restore location. Overwrite? [y/N]")
            IF user_declined:
                CONTINUE
        
        // Move back to original location
        create_dir_all(restore_path.parent())
        rename(trash_file, restore_path)
        
        emit(Restored { path: entry.original_path })
    
    RETURN Ok(())
```

---

### Algorithm 5: Delta Transfer (rsync-style Rolling Checksum)

**Purpose:** For large modified files, transfer only the changed portions.

**Use Case:** 100MB file where only 5MB changed → transfer 5MB instead of 100MB

**Complexity:** O(n) where n = file size
**Phase:** 3 (Remote sync optimization)

```rust
// ═══════════════════════════════════════════════════════════
// REMOTE SIDE: Generate block signatures
// ═══════════════════════════════════════════════════════════
FUNCTION generate_signatures(file_path: Path) -> Vec<BlockSignature>:
    
    BLOCK_SIZE = 4096  // 4KB blocks
    file = open(file_path)
    signatures = []
    block_index = 0
    
    LOOP:
        block = file.read(BLOCK_SIZE)
        IF block.is_empty():
            BREAK
        
        // Compute two checksums per block:
        // 1. Weak (fast): Adler-32 rolling checksum
        // 2. Strong (collision-resistant): Blake3 hash
        
        weak_checksum = adler32(block)
        strong_checksum = blake3(block)
        
        signatures.push(BlockSignature {
            index: block_index,
            weak: weak_checksum,
            strong: strong_checksum
        })
        
        block_index += 1
    
    RETURN signatures


// ═══════════════════════════════════════════════════════════
// LOCAL SIDE: Find matching blocks and generate delta
// ═══════════════════════════════════════════════════════════
FUNCTION generate_delta(
    local_file: Path,
    remote_signatures: Vec<BlockSignature>
) -> Delta:
    
    // Build lookup table: weak_checksum -> [BlockSignature]
    signature_map = HashMap::new()
    FOR sig IN remote_signatures:
        signature_map.entry(sig.weak).push(sig)
    
    file = open(local_file)
    delta = Delta::new()
    window = RollingWindow::new(BLOCK_SIZE)
    
    position = 0
    literal_buffer = []
    
    LOOP:
        byte = file.read_byte()
        IF byte.is_none():
            BREAK
        
        window.push(byte)
        position += 1
        
        // Window full? Check for matches
        IF window.len() == BLOCK_SIZE:
            weak = window.adler32()
            
            // Fast lookup: check weak checksum first
            IF signature_map.contains(weak):
                candidates = signature_map.get(weak)
                
                // Verify with strong checksum
                strong = blake3(window.data())
                
                FOR candidate IN candidates:
                    IF candidate.strong == strong:
                        // MATCH FOUND!
                        
                        // Flush any pending literal data
                        IF !literal_buffer.is_empty():
                            delta.add_literal(literal_buffer)
                            literal_buffer.clear()
                        
                        // Reference existing block
                        delta.add_block_reference(candidate.index)
                        
                        // Skip ahead
                        window.clear()
                        BREAK
            
            // No match: this byte is "new data"
            ELSE:
                literal_buffer.push(window.pop_front())
    
    // Flush remaining literals
    IF !literal_buffer.is_empty():
        delta.add_literal(literal_buffer)
    
    RETURN delta


// ═══════════════════════════════════════════════════════════
// REMOTE SIDE: Reconstruct file from delta
// ═══════════════════════════════════════════════════════════
FUNCTION apply_delta(
    old_file: Path,
    delta: Delta,
    output_file: Path
) -> Result<()>:
    
    old = open(old_file, READ_ONLY)
    new = open(output_file, CREATE | WRITE)
    
    FOR instruction IN delta.instructions:
        MATCH instruction:
            
            // Copy block from old file
            BlockReference(index):
                offset = index * BLOCK_SIZE
                old.seek(offset)
                block = old.read(BLOCK_SIZE)
                new.write(block)
            
            // Write new literal data
            Literal(data):
                new.write(data)
    
    new.flush()
    new.sync_all()
    
    RETURN Ok(())
```

**Efficiency Analysis:**

```
Example: 100MB file, 5MB changed

Without delta transfer:
  - Transfer size: 100MB
  - Time at 10MB/s: 10 seconds

With delta transfer:
  - Signature size: ~1.2MB (25,000 blocks × 50 bytes/block)
  - Delta size: ~5MB (only changed data)
  - Total transfer: 6.2MB
  - Time at 10MB/s: 0.62 seconds
  - Speedup: 16x faster
```

---

### Algorithm 6: Watch Mode (Filesystem Event Monitoring)

**Purpose:** Automatically sync when source files change.

**Strategy:** 
1. Monitor filesystem events (create, modify, delete)
2. Debounce rapid changes (e.g., `git checkout` creates 1000s of events)
3. Batch sync after settle period

```rust
FUNCTION watch_and_sync(
    src_path: Path,
    dest_path: Path,
    config: Config
) -> Result<()>:
    
    // ═══════════════════════════════════════════════════════
    // SETUP: Initialize filesystem watcher
    // ═══════════════════════════════════════════════════════
    watcher = notify::watcher(config.watch_settle)
    watcher.watch(src_path, RecursiveMode::Recursive)
    
    // Debouncing state
    pending_changes = HashSet::new()
    last_event_time = None
    settle_duration = Duration::from_secs(config.watch_settle)
    
    println!("👀 Watching {} for changes...", src_path)
    
    // ═══════════════════════════════════════════════════════
    // EVENT LOOP: Process filesystem events
    // ═══════════════════════════════════════════════════════
    LOOP:
        event = watcher.recv_timeout(Duration::from_millis(100))
        
        MATCH event:
            
            // ───────────────────────────────────────────────
            // File changed
            // ───────────────────────────────────────────────
            Ok(Event::Modify(path)) | Ok(Event::Create(path)):
                pending_changes.insert(path)
                last_event_time = Some(now())
                
                emit(ChangeDetected { path })
            
            // ───────────────────────────────────────────────
            // File deleted
            // ───────────────────────────────────────────────
            Ok(Event::Remove(path)):
                pending_changes.insert(path)
                last_event_time = Some(now())
            
            // ───────────────────────────────────────────────
            // Timeout: Check if settle period elapsed
            // ───────────────────────────────────────────────
            Err(RecvTimeoutError::Timeout):
                IF last_event_time.is_some():
                    elapsed = now() - last_event_time.unwrap()
                    
                    IF elapsed >= settle_duration:
                        // Settle period complete → trigger sync
                        
                        IF !pending_changes.is_empty():
                            emit(Syncing {
                                file_count: pending_changes.len()
                            })
                            
                            // Run incremental sync
                            sync_incremental(
                                src_path,
                                dest_path,
                                pending_changes,
                                config
                            )
                            
                            // Reset state
                            pending_changes.clear()
                            last_event_time = None
                            
                            emit(SyncComplete)
            
            // ───────────────────────────────────────────────
            // User interrupt (Ctrl+C)
            // ───────────────────────────────────────────────
            Err(RecvTimeoutError::Disconnected):
                BREAK


// ═══════════════════════════════════════════════════════════
// Incremental Sync (only changed files)
// ═══════════════════════════════════════════════════════════
FUNCTION sync_incremental(
    src_root: Path,
    dest_root: Path,
    changed_paths: HashSet<Path>,
    config: Config
) -> Result<()>:
    
    // Build mini file trees for only changed files
    src_tree = FileTree::new()
    dest_tree = FileTree::new()
    
    FOR path IN changed_paths:
        relative = path.strip_prefix(src_root)
        
        // Scan source
        IF path.exists():
            src_tree.insert(relative, scan_file(path))
        
        // Scan destination
        dest_path = dest_root.join(relative)
        IF dest_path.exists():
            dest_tree.insert(relative, scan_file(dest_path))
    
    // Use standard diff engine
    plan = generate_sync_plan(src_tree, dest_tree, config)
    
    // Execute plan
    execute_plan(plan, config)
    
    RETURN Ok(())
```

---

## Data Structures

### Core Types

```rust
// ═══════════════════════════════════════════════════════════
// File Entry (represents a single file)
// ═══════════════════════════════════════════════════════════
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative path from sync root
    pub path: PathBuf,
    
    /// File size in bytes
    pub size: u64,
    
    /// Last modification time (UTC)
    pub mtime: SystemTime,
    
    /// Unix permissions (mode bits)
    pub permissions: u32,
    
    /// Blake3 content hash (computed lazily)
    pub hash: Option<[u8; 32]>,
    
    /// Symlink metadata
    pub is_symlink: bool,
    pub symlink_target: Option<PathBuf>,
}

// ═══════════════════════════════════════════════════════════
// File Tree (directory structure)
// ═══════════════════════════════════════════════════════════
#[derive(Debug, Clone)]
pub struct FileTree {
    /// Map: relative_path -> FileEntry
    pub entries: HashMap<PathBuf, FileEntry>,
    
    /// Aggregate statistics
    pub total_size: u64,
    pub total_files: usize,
    pub total_dirs: usize,
    
    /// Scan metadata
    pub scan_duration: Duration,
    pub root_path: PathBuf,
}

// ═══════════════════════════════════════════════════════════
// Sync Actions (diff engine output)
// ═══════════════════════════════════════════════════════════
#[derive(Debug, Clone)]
pub enum SyncAction {
    /// Copy new file (src exists, dest missing)
    CopyNew(FileEntry),
    
    /// Overwrite existing file (content differs)
    Overwrite(FileEntry),
    
    /// Delete file (dest exists, src missing)
    Delete(PathBuf),
    
    /// Move/rename optimization (Phase 3)
    Move { from: PathBuf, to: PathBuf },
    
    /// Skip (files identical)
    Skip,
}

// ═══════════════════════════════════════════════════════════
// Configuration
// ═══════════════════════════════════════════════════════════
#[derive(Debug, Clone)]
pub struct Config {
    // Paths
    pub source: PathBuf,
    pub destination: PathBuf,
    
    // Behavior flags
    pub dry_run: bool,
    pub checksum_mode: bool,
    pub delete_mode: DeleteMode,
    
    // Filtering
    pub exclude: Vec<String>,  // Glob patterns
    pub include: Vec<String>,
    
    // Performance
    pub threads: usize,
    pub bandwidth_limit: Option<u64>,  // bytes/sec
    
    // Advanced features
    pub backup_dir: Option<PathBuf>,
    pub watch: bool,
    pub watch_settle: u64,  // seconds
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeleteMode {
    None,       // Don't delete anything
    Trash,      // Move to .kopy_trash/
    Permanent,  // Unlink (dangerous!)
}

// ═══════════════════════════════════════════════════════════
// Events (for progress reporting)
// ═══════════════════════════════════════════════════════════
#[derive(Debug, Clone)]
pub enum SyncEvent {
    // Scanning
    ScanStart { path: PathBuf },
    ScanProgress { files: usize, size: u64 },
    ScanComplete { tree: FileTree },
    
    // Transfer
    FileStart { path: PathBuf, size: u64 },
    Progress { bytes: u64 },
    FileComplete { path: PathBuf },
    
    // Errors
    Error { path: PathBuf, error: String },
    Conflict { path: PathBuf, reason: String },
}

// ═══════════════════════════════════════════════════════════
// Errors
// ═══════════════════════════════════════════════════════════
#[derive(Debug, thiserror::Error)]
pub enum KopyError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Permission denied: {path}")]
    PermissionDenied { path: PathBuf },
    
    #[error("Disk full: {available} bytes available, {needed} needed")]
    DiskFull { available: u64, needed: u64 },
    
    #[error("Checksum mismatch: {path}")]
    ChecksumMismatch { path: PathBuf },
    
    #[error("Transfer interrupted: {path} at offset {offset}")]
    TransferInterrupted { path: PathBuf, offset: u64 },
    
    #[error("SSH connection failed: {0}")]
    SshError(String),
}
```

---

## Module Structure

```
kopy/
├── src/
│   ├── main.rs              # CLI entry point, argument parsing
│   ├── lib.rs               # Public API exports
│   │
│   ├── config/
│   │   ├── mod.rs           # Config struct and defaults
│   │   ├── profile.rs       # Profile loading from TOML
│   │   └── exclude.rs       # Pattern matching (.gitignore)
│   │
│   ├── scanner/
│   │   ├── mod.rs           # Directory scanning logic
│   │   ├── walker.rs        # Sequential walker (Phase 1)
│   │   ├── parallel.rs      # Parallel walker (Phase 2)
│   │   └── filter.rs        # Exclude/include filtering
│   │
│   ├── diff/
│   │   ├── mod.rs           # Diff engine entry point
│   │   ├── compare.rs       # File comparison logic
│   │   ├── plan.rs          # SyncAction plan generation
│   │   └── conflict.rs      # Conflict detection
│   │
│   ├── executor/
│   │   ├── mod.rs           # Execution coordinator
│   │   ├── copy.rs          # Atomic file copy
│   │   ├── trash.rs         # Trash-based delete
│   │   ├── pool.rs          # Thread pool (Phase 2)
│   │   └── resume.rs        # Resume capability
│   │
│   ├── hash/
│   │   ├── mod.rs           # Hashing utilities
│   │   ├── blake3.rs        # Blake3 implementation
│   │   └── rolling.rs       # Rolling checksum (Phase 3)
│   │
│   ├── remote/
│   │   ├── mod.rs           # Remote sync coordinator (Phase 3)
│   │   ├── ssh.rs           # SSH client
│   │   ├── agent.rs         # Agent mode protocol
│   │   └── delta.rs         # Delta transfer algorithm
│   │
│   ├── watch/
│   │   ├── mod.rs           # Watch mode (Phase 3)
│   │   └── debounce.rs      # Event debouncing
│   │
│   ├── ui/
│   │   ├── mod.rs           # UI coordinator
│   │   ├── progress.rs      # Progress bars (indicatif)
│   │   ├── reporter.rs      # Event → UI mapping
│   │   └── interactive.rs   # Conflict prompts
│   │
│   ├── commands/
│   │   ├── mod.rs           # Subcommand dispatcher
│   │   ├── sync.rs          # Main sync command
│   │   ├── verify.rs        # Verification command
│   │   └── trash.rs         # Trash management
│   │
│   └── types/
│       ├── mod.rs           # Core type exports
│       ├── entry.rs         # FileEntry
│       ├── tree.rs          # FileTree
│       ├── action.rs        # SyncAction
│       └── error.rs         # KopyError
│
├── tests/
│   ├── integration/         # End-to-end tests
│   ├── fixtures/            # Test data
│   └── benchmarks/          # Performance tests
│
├── Cargo.toml
└── README.md
```

---

## Implementation Phases

### Phase 1: MVP (4-6 weeks)

**Goal:** Working local sync with core safety features

#### Week 1-2: Foundation
- [ ] Project setup (Cargo.toml, dependencies)
- [ ] CLI argument parsing with `clap`
- [ ] Config struct and validation
- [ ] Basic error types
- [ ] Directory scanner (sequential `walkdir`)
- [ ] FileEntry and FileTree types

#### Week 3-4: Core Logic
- [ ] Diff engine (metadata comparison only)
- [ ] SyncAction plan generation
- [ ] Single-threaded file copy
- [ ] Atomic operations (`.part` files)
- [ ] Trash-based delete implementation
- [ ] Exclude pattern matching

#### Week 5-6: UX & Testing
- [ ] Simple progress bar (single file)
- [ ] Dry-run mode
- [ ] Error handling and reporting
- [ ] Unit tests for diff engine
- [ ] Integration tests (basic sync)
- [ ] Documentation

**Deliverable:** `kopy src/ dest/` works reliably for local directories

---

### Phase 2: Performance (3-4 weeks)

**Goal:** Make it fast and robust

#### Week 7-8: Parallelization
- [ ] Parallel directory walking (`jwalk`)
- [ ] Thread pool for concurrent transfers
- [ ] Event-driven progress reporting
- [ ] Multi-file progress display (indicatif)
- [ ] Bandwidth limiting

#### Week 9-10: Robustness
- [ ] Blake3 hashing implementation
- [ ] Checksum mode (`--checksum`)
- [ ] Resume capability (detect `.part` files)
- [ ] Conflict detection (dest newer than src)
- [ ] Interactive conflict resolution
- [ ] Error summary report
- [ ] Comprehensive error messages

**Deliverable:** Fast, resumable syncs with clear progress

---

### Phase 3: Remote & Advanced (4-6 weeks)

**Goal:** SSH support and pro features

#### Week 11-13: Remote Sync
- [ ] SSH client integration (`ssh2`)
- [ ] Agent mode protocol
- [ ] Binary serialization (`bincode`)
- [ ] Remote manifest exchange
- [ ] SFTP fallback mode
- [ ] Delta transfer algorithm (rolling checksum)

#### Week 14-16: Advanced Features
- [ ] Watch mode (`notify` + debouncing)
- [ ] Snapshot backups (`--backup-dir`)
- [ ] Profile system (TOML config loading)
- [ ] Rename detection heuristic
- [ ] Verify subcommand
- [ ] Trash management commands

**Deliverable:** Full-featured rsync replacement

---

### Phase 4: Polish (2-3 weeks)

**Goal:** Production-ready UX

#### Week 17-18: Quality
- [ ] Comprehensive error messages
- [ ] Shell completions (bash, zsh, fish)
- [ ] Man page generation
- [ ] Benchmarking suite
- [ ] Performance profiling
- [ ] Memory optimization

#### Week 19: Release Prep
- [ ] Documentation (README, examples)
- [ ] CI/CD setup (GitHub Actions)
- [ ] Release builds (cross-compilation)
- [ ] Changelog
- [ ] 1.0 announcement

**Deliverable:** 1.0 release

---

## Verification Plan

### Automated Tests

#### Unit Tests
```bash
# Run all unit tests
cargo test --lib

# Test specific modules
cargo test scanner::
cargo test diff::
cargo test executor::
```

**Coverage targets:**
- Diff engine: 100% (critical logic)
- File operations: 90%
- Overall: 80%

#### Integration Tests
```bash
# Run integration test suite
cargo test --test integration

# Specific scenarios
cargo test test_basic_sync
cargo test test_resume_capability
cargo test test_trash_restore
```

**Test scenarios:**
1. Basic sync (empty dest)
2. Update existing files
3. Delete mode (trash)
4. Exclude patterns
5. Conflict handling
6. Resume interrupted transfer
7. Checksum verification
8. Watch mode

#### Benchmarks
```bash
# Run performance benchmarks
cargo bench

# Compare with rsync
./scripts/benchmark_vs_rsync.sh
```

**Benchmark targets:**
- 10,000 small files (1KB each)
- 100 large files (100MB each)
- Mixed workload (realistic)

### Manual Verification

#### Phase 1 Checklist
- [ ] Sync 1GB directory successfully
- [ ] Verify dry-run shows correct plan
- [ ] Test trash restore functionality
- [ ] Confirm exclude patterns work
- [ ] Interrupt transfer and verify `.part` file exists

#### Phase 2 Checklist
- [ ] Sync 100GB directory with parallel mode
- [ ] Verify checksum mode detects corruption
- [ ] Test resume on interrupted large file
- [ ] Confirm conflict detection works
- [ ] Check bandwidth limiting accuracy

#### Phase 3 Checklist
- [ ] SSH sync to remote server
- [ ] Verify delta transfer efficiency
- [ ] Test watch mode with rapid changes
- [ ] Confirm snapshot backups work
- [ ] Load and use profile config

---

## Risk Mitigation

### Critical Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Data loss from bugs** | Critical | Extensive testing, trash-based deletes, dry-run defaults |
| **Performance worse than rsync** | High | Benchmarking, profiling, parallel operations |
| **Cross-platform issues** | Medium | CI testing on Linux/macOS/Windows |
| **SSH security vulnerabilities** | High | Use battle-tested `ssh2` crate, audit dependencies |
| **Scope creep** | Medium | Strict phase boundaries, MVP-first approach |

### Edge Cases to Handle

1. **Filesystem limits:**
   - Path length limits (Windows: 260 chars)
   - Filename character restrictions
   - Case-insensitive filesystems (macOS)

2. **Permissions:**
   - Read-only files
   - Permission denied on destination
   - Setuid/setgid bits

3. **Special files:**
   - Symlinks (circular, broken)
   - Named pipes (FIFOs)
   - Device files
   - Sockets

4. **Resource constraints:**
   - Disk full during transfer
   - Out of memory (huge directories)
   - Too many open files

---

## Success Criteria

### Phase 1 (MVP)
- ✅ Successfully syncs 10,000 files without errors
- ✅ Trash restore works 100% of the time
- ✅ Dry-run accurately predicts actions
- ✅ No data corruption on interrupt (`.part` files)

### Phase 2 (Performance)
- ✅ 2x faster than rsync for 10,000 small files
- ✅ Resume works after interrupt at any point
- ✅ Checksum mode catches all corruption
- ✅ Bandwidth limiting within 5% of target

### Phase 3 (Remote)
- ✅ SSH sync works with standard OpenSSH servers
- ✅ Delta transfer reduces bandwidth by 80%+ for incremental updates
- ✅ Watch mode detects changes within 2 seconds
- ✅ Profiles load and work correctly

### Phase 4 (Polish)
- ✅ Zero critical bugs in issue tracker
- ✅ Documentation complete and clear
- ✅ 100+ GitHub stars (community validation)
- ✅ Positive feedback from beta testers

---

## Dependencies

### Rust Crates

```toml
[dependencies]
# CLI & Config
clap = { version = "4.5", features = ["derive", "color"] }
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"

# Async Runtime
tokio = { version = "1.40", features = ["full"] }

# File System
walkdir = "2.5"           # Phase 1: Sequential walker
jwalk = "0.8"             # Phase 2: Parallel walker
camino = "1.1"            # UTF-8 paths
notify = "6.1"            # Phase 3: Filesystem events

# Hashing
blake3 = "1.5"            # Fast, parallel hashing

# Serialization
bincode = "1.3"           # Phase 3: Network protocol

# Pattern Matching
globset = "0.4"           # .gitignore style patterns

# Progress & UI
indicatif = "0.17"        # Progress bars
console = "0.15"          # Terminal colors

# SSH (Phase 3)
ssh2 = "0.9"              # SSH client

# Error Handling
thiserror = "1.0"         # Error derive macros
anyhow = "1.0"            # Error context

[dev-dependencies]
criterion = "0.5"         # Benchmarking
tempfile = "3.8"          # Temp directories for tests
assert_cmd = "2.0"        # CLI testing
predicates = "3.0"        # Test assertions
```

---

## Timeline Summary

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| Phase 1: MVP | 4-6 weeks | Working local sync |
| Phase 2: Performance | 3-4 weeks | Fast, robust sync |
| Phase 3: Remote | 4-6 weeks | SSH + advanced features |
| Phase 4: Polish | 2-3 weeks | 1.0 release |
| **Total** | **13-19 weeks** | **Production-ready tool** |

---

## Next Steps

1. **Initialize project:**
   ```bash
   cargo new kopy --bin
   cd kopy
   ```

2. **Set up dependencies** (copy Cargo.toml from this plan)

3. **Create module structure** (see Module Structure section)

4. **Begin Phase 1, Week 1:**
   - CLI argument parsing
   - Config struct
   - Basic types (FileEntry, FileTree)

5. **Write first test:**
   ```rust
   #[test]
   fn test_scan_empty_directory() {
       let temp = TempDir::new().unwrap();
       let tree = scan_directory(temp.path()).unwrap();
       assert_eq!(tree.total_files, 0);
   }
   ```

---

**Status:** 📋 Plan complete, ready for implementation

**Target Start:** February 2026

**Target 1.0:** Q2 2026

---

*This implementation plan is a living document. Update as implementation progresses and requirements evolve.*
