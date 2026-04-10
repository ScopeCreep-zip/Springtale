---
paths:
  - "crates/**/*.rs"
  - "crates/**/Cargo.toml"
---

# Crate Structure Guidelines

## Each crate follows this pattern:

```
crates/springtale-{name}/
├── Cargo.toml          # workspace = true for shared deps
└── src/
    ├── lib.rs          # pub mod declarations + re-exports only
    ├── {module}/
    │   ├── mod.rs      # pub use re-exports
    │   ├── types.rs    # Data types
    │   ├── trait_.rs   # Trait definitions (if any)
    │   └── impl.rs     # Implementations
    └── error.rs        # Crate-level error types (thiserror)
```

## Crate Dependency Rules

Internal crate dependencies flow downward only:

```
springtale-core          (zero deps on other springtale crates)
springtale-crypto        (zero deps on other springtale crates)
springtale-transport     (depends on: crypto)
springtale-store         (depends on: core)
springtale-connector     (depends on: crypto, store)
springtale-scheduler     (depends on: core, store)
springtale-ai            (depends on: core)
springtale-mcp           (depends on: core, connector)
springtale-runtime       (depends on: core, crypto, store, connector, ai, sentinel)
springtale-bot           (depends on: core, crypto, connector, store, transport, ai, runtime)
springtale-sentinel      (depends on: core, store)
```

No circular dependencies. No upward dependencies. If you need a type from a higher crate, it belongs in a lower crate.

## Crate Cargo.toml Template

```toml
[package]
name = "springtale-{name}"
version = "0.1.0"
edition = "2024"

[dependencies]
# Use workspace deps:
serde = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }

# Internal crates (only what's needed):
springtale-core = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
```

## lib.rs Template

`lib.rs` is a table of contents. It declares modules and re-exports. Nothing else.
No functions. No types. No impl blocks. No constants.

```rust
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod module_a;
pub mod module_b;
pub mod error;

// Re-exports for convenience (optional)
pub use error::MyError;
pub use module_a::MyType;

// NOTHING ELSE GOES HERE.
// Every function, type, trait, impl, and constant lives in a module file.
```
