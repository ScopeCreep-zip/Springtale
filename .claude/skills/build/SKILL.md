---
name: build
description: Build the entire Springtale workspace and report results
allowed-tools: Bash, Read
---

Build the Springtale workspace:

1. Run `cargo build --workspace 2>&1` and capture output
2. If build fails, read the error, identify the failing crate, and report what needs fixing
3. If build succeeds, run `cargo clippy --workspace --all-targets -- -D warnings 2>&1`
4. Report: which crates built, any warnings, any clippy issues
