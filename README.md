# kopy

## Overview
`kopy` is a Rust CLI for local directory synchronization.

It is built for developers and power users who want safer defaults than classic sync tools: atomic file writes, dry-run planning, and trash-first deletes. The project started as a practical response to common `rsync` pain points (easy-to-miss flags, risky deletes, and weak error summaries) while keeping day-to-day usage simple.

## Features
- Local source -> destination sync for files and directories.
- Plan generation with action counts (`copy`, `update`, `delete`, `skip`).
- `--dry-run` mode that shows planned actions without changing files.
- Safe delete mode (`--delete`) that moves removed files to `.kopy_trash`.
- Permanent delete option (`--delete-permanent`) when explicitly requested.
- Include/exclude filtering (`--include`, `--exclude`) plus `.gitignore`/`.kopyignore`.
- Optional checksum validation (`--checksum`) using Blake3.
- Progress output for scanning and transfer phases, including throughput.
- Error summary with grouped, plain-English messages.

## Installation
```bash
git clone git@github.com:SwagRGB/kopy.git
cd kopy

cargo build --release
cargo install --path .
```

To run without installing:
```bash
cargo run -- <source> <destination> [flags]
```

## Usage
```bash
# Basic sync
kopy ./src_dir ./backup_dir

# Preview only
kopy ./src_dir ./backup_dir --dry-run

# Safe delete (moves destination-only files to .kopy_trash)
kopy ./src_dir ./backup_dir --delete

# Exclude temp files
kopy ./src_dir ./backup_dir --exclude "*.tmp" --exclude "node_modules/**"
```

## Configuration
Main flags:
- `--dry-run`
- `--checksum`
- `--delete` or `--delete-permanent` (mutually exclusive)
- `--exclude <glob>` (repeatable)
- `--include <glob>` (repeatable, overrides matching excludes)

Notes:
- Source must exist and be a file or directory.
- Source and destination cannot be equal or nested within each other.

## Limitations
- Current scope is local filesystem sync (no remote/SSH sync yet).
- Execution is currently single-threaded; parallel transfer is planned.
- Dry-run output is intentionally concise and may omit unchanged file paths.
- Linux-only runtime support (Windows/macOS are currently out of scope).

## Contributing
```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Please keep changes focused, add tests for behavior changes, and use conventional commit messages (for example `feat(sync): ...`, `fix(executor): ...`).

## License
MIT (see `LICENSE`).
