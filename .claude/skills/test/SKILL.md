---
name: test
description: Run the full test suite and report results
allowed-tools: Bash, Read
---

Run the Springtale test suite:

1. Run `cargo nextest run --workspace 2>&1` (or `cargo test --workspace` if nextest unavailable)
2. If any tests fail, identify the failing tests and crates
3. Run `cargo test --doc 2>&1` for doc tests
4. Report: total tests, passed, failed, and which crates have failures
