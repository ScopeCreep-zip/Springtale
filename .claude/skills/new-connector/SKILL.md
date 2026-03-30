---
name: new-connector
description: Scaffold a new first-party connector crate with the standard structure
allowed-tools: Bash, Write, Read, Edit, Glob
---

Scaffold a new connector. The user provides the connector name (e.g., "kick", "telegram").

1. Read `docs/current-arch/ARCHITECTURE.md` sections on the specific connector for design requirements
2. Read `.claude/rules/connector-guidelines.md` for the standard structure
3. Create the directory structure:
   ```
   connectors/connector-{name}/
   ├── Cargo.toml
   └── src/
       ├── lib.rs
       ├── config.rs
       ├── error.rs
       ├── auth/
       │   └── mod.rs
       ├── client/
       │   ├── mod.rs
       │   └── api.rs
       ├── triggers/
       │   └── mod.rs
       └── actions/
           └── mod.rs
   ```
4. Add the crate to workspace members in root `Cargo.toml`
5. Implement the `Connector` trait stub
6. Create a `connector-{name}.toml` manifest stub with capability declarations
7. Add basic tests
