---
description: The procedure for initializing a new development phase (e.g., moving from Phase 1 to Phase 2), involving dependency management and structural scaffolding.
---

1.  **Dependency Management**:
    * Open `implementation_plan.md` and read the "Dependencies" section for the requested Phase.
    * Update `Cargo.toml` to include these specific versions.
    * *Constraint*: Do not add dependencies not listed in the plan without asking the user.
2.  **Scaffolding**:
    * Read the "Module Structure" tree in `implementation_plan.md`.
    * Create the required directory hierarchy and empty `mod.rs` files.
    * Ensure `main.rs` or `lib.rs` exposes these new modules.
3.  **Sanity Check**:
    * Run `cargo check` to ensure the new dependencies resolve correctly before writing any logic.


