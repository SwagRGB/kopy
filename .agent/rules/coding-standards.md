---
trigger: always_on
---

# Coding Standards (Rust)
- **Safety First:** NEVER use `.unwrap()` or `.expect()` in production code. Use proper `Result` propagation with `?`.
- **Error Handling:** Use `thiserror` for libraries and `anyhow` for the CLI binary. Ensure every error provides context (e.g., *which* file failed, *why* it failed).
- **Type System:** Use the "New Type" pattern and Enums to make invalid states unrepresentable.
- **Concurrency:** Prefer message passing (`mpsc`) over shared state (`Mutex`) where possible.
- **Async:** Use `tokio` for I/O-bound tasks.
- **Comments:** Document all public structs and functions with `///` doc comments.

# Architectural Constraints
- **Phase adherence:** Do not implement features from Phase 3 (Remote) if we are currently working on Phase 1 (MVP).
- **Dependency discipline:** Only use crates listed in `implementation_plan.md`. Do not add new dependencies without asking.
- **Algorithm fidelity:** When implementing logic (e.g., "Smart Discovery"), verify your code against the pseudocode in `implementation_plan.md`.

# Critical UX Rules
- Output must be human-readable and plain English.
- "Delete" operations must ALWAYS default to "Move to Trash" (unless the `Permanent` flag is strictly set).
- Dry-run must be the most polished feature of the tool.
