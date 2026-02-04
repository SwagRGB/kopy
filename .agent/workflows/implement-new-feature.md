---
description: The standard operating procedure for building any feature listed in the project roadmap. This ensures strict adherence to the architectural specs and Test-Driven Development (TDD) principles.
---

1.  **Context Retrieval**:
    * Locate the specific task in `task.md` to understand the scope.
    * Search `implementation_plan.md` for the matching **Algorithm** and **Data Structure** definitions.
    * *Constraint*: Do not deviate from the pseudocode or struct definitions in the plan without explicit user override.
2.  **Type-First Design**:
    * Define the necessary `struct`s and `enum`s in the appropriate module before writing logic.
    * Verify these types match the `implementation_plan.md` specs.
3.  **TDD (Test Driven Development)**:
    * Create a new test file or `mod tests` block.
    * Write a *failing* unit test that asserts the desired behavior (e.g., "ensure file is moved to trash").
4.  **Implementation**:
    * Write the minimum Rust code required to pass the test.
    * *Constraint*: Use `tokio` for I/O and ensure proper `Result` propagation (no `unwrap()`).
5.  **Verification**:
    * Run `cargo test` to confirm functionality.
    * Run `cargo clippy` to ensure idiomatic Rust.
    * Update `task.md` by marking the relevant checklist item as `[x]`.
