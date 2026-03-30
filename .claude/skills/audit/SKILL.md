---
name: audit
description: Run security audit checks on the workspace
allowed-tools: Bash, Read, Grep
---

Run the Springtale security audit pipeline:

1. `cargo deny check 2>&1` — license + advisory policy
2. `cargo audit 2>&1` — RustSec advisory DB
3. `cargo clippy --workspace --all-targets -- -D warnings 2>&1` — lint security checks
4. Search for any `expose_secret()` calls without `// SECURITY:` annotation
5. Search for any `unsafe` blocks without `// SAFETY:` annotation
6. Check that no crate imports `native-tls` or `openssl`
7. Report findings grouped by severity
