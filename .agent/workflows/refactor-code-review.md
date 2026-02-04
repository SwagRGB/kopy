---
description: A rigid audit process to ensure code safety, performance, and user experience standards are met before commiting.
---

1.  **Safety Audit**:
    * Scan strictly for `unwrap()`, `expect()`, or `panic!` usage. Flag them as critical errors unless they are in test code.
    * Check for race conditions (TOCTOU) in file operations. Ensure "Check" and "Act" are atomic or handled via temporary files (`.part`).
2.  **Performance Audit**:
    * Identify blocking I/O operations inside `async` functions.
    * Flag unnecessary `.clone()` calls on potentially large data structures (like `FileTree`).
3.  **UX Audit**:
    * Review all error strings. Ensure they offer a "reason" and a "solution" (e.g., "Permission denied: Try checking file ownership" instead of just "Error 13").
    * Verify that "Delete" operations default to the Trash mechanism, not permanent unlink.
