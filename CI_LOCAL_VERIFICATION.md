# CI/CD Local Verification Guide

## Quick Reference

Run these commands before pushing to ensure CI will pass:

```bash
# 1. Format check
cargo fmt --check

# 2. Lint check (zero warnings tolerance)
cargo clippy --all-targets --all-features -- -D warnings

# 3. Run tests
cargo test
```

## Detailed Commands

### 1. Format Check (`cargo fmt`)

**What it does:** Ensures code follows Rust's standard formatting style.

**Check without modifying:**
```bash
cargo fmt --check
```

**Auto-fix formatting issues:**
```bash
cargo fmt
```

**Expected output (success):**
```
# No output = all files formatted correctly
```

---

### 2. Clippy Lints (`cargo clippy`)

**What it does:** Runs Rust's linter to catch common mistakes and enforce idiomatic code.

**Zero-tolerance mode (matches CI):**
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

**Explanation of flags:**
- `--all-targets` - Check lib, bins, tests, benches, examples
- `--all-features` - Enable all feature flags
- `-- -D warnings` - **Treat all warnings as errors** (zero tolerance)

**Expected output (success):**
```
    Checking kopy v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.14s
```

**Common fixes:**
- Unused imports: Remove them
- Derivable impls: Use `#[derive(...)]` instead of manual `impl`
- Dead code: Remove or mark with `#[allow(dead_code)]` if intentional

---

### 3. Run Tests (`cargo test`)

**What it does:** Runs all unit tests, integration tests, and doc tests.

**Run all tests:**
```bash
cargo test
```

**Run with verbose output:**
```bash
cargo test --verbose
```

**Run specific test:**
```bash
cargo test test_name
```

**Expected output (success):**
```
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

---

## Complete Pre-Push Checklist

Run this one-liner to verify everything:

```bash
cargo fmt --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test && \
echo "✅ All CI checks passed! Safe to push."
```

**If any step fails:**
1. Read the error message carefully
2. Fix the issue
3. Run the failing command again
4. Repeat until all pass

---

## CI Pipeline Details

The GitHub Actions workflow (`.github/workflows/ci.yml`) runs on:
- **Trigger:** Push or PR to `main` or `develop` branches
- **OS:** `ubuntu-latest`
- **Rust:** Stable toolchain with `rustfmt` and `clippy` components

**Pipeline steps:**
1. Checkout code
2. Install Rust toolchain
3. Cache dependencies (for faster builds)
4. **Check formatting** - `cargo fmt --check`
5. **Run clippy** - `cargo clippy -- -D warnings`
6. **Run tests** - `cargo test --verbose`
7. **Build release** - `cargo build --release --verbose`

---

## Troubleshooting

### "cargo fmt --check" fails
```bash
# Fix automatically
cargo fmt

# Verify
cargo fmt --check
```

### "cargo clippy" shows warnings
```bash
# See detailed suggestions
cargo clippy --all-targets --all-features

# Fix and re-run with zero tolerance
cargo clippy --all-targets --all-features -- -D warnings
```

### Tests fail
```bash
# Run with backtrace for debugging
RUST_BACKTRACE=1 cargo test

# Run specific test with output
cargo test test_name -- --nocapture
```

---

## Best Practices

1. **Run checks frequently** - Don't wait until commit time
2. **Fix warnings immediately** - Zero-tolerance policy prevents accumulation
3. **Write tests first** - TDD approach (per `/implement-new-feature` workflow)
4. **Use `cargo watch`** - Auto-run checks on file changes:
   ```bash
   cargo install cargo-watch
   cargo watch -x fmt -x clippy -x test
   ```

---

**Status:** ✅ CI/CD pipeline configured and verified
