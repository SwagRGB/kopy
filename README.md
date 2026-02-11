# kopy - Modern File Synchronization Tool

> **"Safety by default, speed by design."**

**Status:** Under Active Development (Phase 1 - MVP)

---

## Overview

kopy is a next-generation CLI synchronization tool designed to replace `rsync` with human-centric design, bulletproof safety guarantees, and zero-configuration operation.

### Why kopy?

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

---

## Core Values

- **Safety**: Trash-can deletes, atomic operations, dry-run previews
- **Clarity**: Plain English output, real-time progress, error summaries
- **Speed**: Parallel transfers, smart hashing, resumable operations
- **Zero Config**: Works out of the box, profiles for advanced use

---

## Development Status

### Phase 1: MVP (In Progress)

`kopy` is currently a functional local sync CLI in active Phase 1 development.

**Implemented today (stable for local workflows):**
- Local source → destination synchronization
- Metadata and optional checksum (`--checksum`) comparison
- Safe delete flow (`--delete` moves files to `.kopy_trash`)
- Explicit permanent delete mode (`--delete-permanent`)
- Include/exclude filtering (`--exclude`, `--include`) plus `.gitignore`/`.kopyignore`
- Symlink-preserving sync behavior
- End-to-end execution engine with continue-on-error handling
- Real-time terminal progress (scan, transfer, current file, throughput)

**Still in progress for Phase 1:**
- Cleaner plan preview UX
- More polished dry-run UX output
- Rich grouped error summaries

**Target:** `kopy src/ dest/` works reliably for local directories

### Future Phases

- **Phase 2:** Performance (parallel transfers, adaptive time estimation, resume capability)
- **Phase 3:** Remote sync (SSH support, delta transfers, watch mode, network latency estimation)
- **Phase 4:** Polish (comprehensive error messages, shell completions, 1.0 release)

---

## Installation

**Note:** kopy is not yet ready for production use. The project is under active development.

### Building from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/kopy.git
cd kopy

# Build with Cargo
cargo build --release

# Run tests
cargo test

# Install locally
cargo install --path .
```

---

## Usage (Preview)

**Note:** The commands below are implemented for local sync workflows (Phase 1). Remote/watch examples remain future-phase.

### Basic Synchronization

```bash
# Simple copy
kopy photos/ /mnt/backup/photos/

# Dry run first (see what would happen)
kopy photos/ /mnt/backup/photos/ --dry-run

# With deletes (safe mode - moves to trash)
kopy photos/ /mnt/backup/photos/ --delete

# Paranoid mode (verify every byte)
kopy photos/ /mnt/backup/photos/ --checksum
```

### Progress Output (Current)

```text
⠋ Scanning source... 124 files | 42.6 MiB
⠋ Scanning destination... 98 files | 37.1 MiB

Plan:
  Copy: 8  Update: 3  Delete: 2  Skip: 89
  Total bytes to transfer: 12451840

[=====>------------------------] 5/11 files | 8.1 MiB transferred | 5.2 MiB/s
Current: Copy docs/report.pdf
```

### Advanced Usage

```bash
# Exclude patterns
kopy src/ dest/ --exclude "*.tmp" --exclude "node_modules/"

# Include overrides exclude
kopy src/ dest/ --exclude "*.log" --include "important.log"

# Watch and auto-sync (Phase 3, not implemented yet)
kopy ~/code/project/ /mnt/dev-backup/project/ --watch

# Remote sync (Phase 3, not implemented yet)
kopy ~/photos/ user@nas:/backup/photos/
```

---

## Safety Guarantees

### Default: Non-Destructive

```bash
kopy photos/ backup/photos/
# Won't delete anything in destination
```

### Delete Mode: Trash-Can Recovery

```bash
kopy photos/ backup/photos/ --delete
# Deleted files go to backup/photos/.kopy_trash/2026-02-04_143022/
```

### Permanent Delete: Explicit Only

```bash
kopy photos/ backup/photos/ --delete-permanent
# Requires explicit flag (dangerous)
```

---

## Technical Stack

- **Language:** Rust (for safety, performance, and reliability)
- **CLI Framework:** clap (derive macros for clean argument parsing)
- **Async Runtime:** tokio (for I/O-bound operations)
- **Hashing:** Blake3 (fast, parallel, cryptographically secure)
- **Pattern Matching:** globset (for .gitignore-style patterns)
- **Progress UI:** indicatif (for beautiful progress bars)

---

## Contributing

kopy is in early development. Contributions, feedback, and bug reports are welcome!

### Development Setup

```bash
# Clone and build
git clone https://github.com/yourusername/kopy.git
cd kopy
cargo build

# Run tests
cargo test

# Run linter
cargo clippy -- -D warnings

# Format code
cargo fmt
```

### Project Structure

```
kopy/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Library root
│   ├── config/           # Configuration and CLI parsing
│   ├── types/            # Core data structures
│   ├── scanner/          # Directory traversal
│   ├── diff/             # Comparison engine
│   ├── executor/         # File operations
│   ├── hash/             # Hashing utilities
│   └── ui/               # Progress and reporting
```

---

## License

MIT License - See LICENSE file for details

---

## Acknowledgments

Inspired by `rsync`, but designed for modern workflows with safety and usability as first-class concerns.

---

**Note:** This project is under active development. The API and features are subject to change. Not recommended for production use until version 1.0.
