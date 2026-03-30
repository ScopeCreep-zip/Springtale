---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
---

# Rust Conventions

## Naming
- Files/modules: `snake_case`
- Types/traits: `PascalCase`
- Functions/methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Trait files: `trait_.rs` (trailing underscore avoids keyword conflict)

## Module Structure (STRICT — this is an architectural constraint, not a style preference)

**Modules over inline. Always.**

- `lib.rs` contains ONLY `pub mod` declarations, re-exports, and lint attributes. No functions. No types. No impl blocks. No constants. Nothing else.
- Every public function, type, trait, error enum, and constant lives in a named module file.
- No free-floating `impl` blocks at crate root. If a type is defined in `types.rs`, its `impl` lives there too.
- No inline type aliases in function signatures. Define named types in modules.
- One type per file when the type has significant impl blocks or more than ~50 lines.
- `mod.rs` re-exports the module's public API and nothing else.
- Prefer deep module trees over wide files. A 500-line file should be split into submodules.
- Helper functions go in the module that uses them, not in a catch-all `utils.rs`.

**Why:** Every public surface is deliberate. When you read `lib.rs` you see the full public API shape at a glance. When you read a module file you see one focused concern. No surprises. No hidden items. This is how we keep a 10-crate workspace navigable.

**Example — correct:**
```
crates/springtale-core/src/
├── lib.rs              # pub mod pipeline; pub mod rule; pub mod router; pub mod transform;
├── pipeline/
│   ├── mod.rs          # pub use context::PipelineContext; pub use stage::Stage;
│   ├── context.rs      # PipelineContext struct + impl
│   ├── stage.rs        # Stage trait
│   ├── compose.rs      # compose_pipeline() function
│   └── error.rs        # PipelineError enum
```

**Example — wrong:**
```
crates/springtale-core/src/
├── lib.rs              # 800 lines with structs, impls, functions, everything inline
```

## Error Handling
- Library crates: `thiserror` for typed error enums. Never `anyhow`.
- App binaries (springtaled, springtale-cli): `anyhow` is fine.
- Never `unwrap()`, `expect()`, or `panic!()` in library code. Enforced by clippy deny.
- Use `?` operator. Return `Result<T, E>` from all fallible functions.

## Async
- All async code uses `tokio`. No `async-std`.
- Use `#[async_trait]` from `async-trait` crate for trait methods.
- Prefer `tokio::select!` for concurrent operations.

## Serialization
- `serde::Deserialize` only on config structs (never `Serialize` — prevents secrets in logs).
- `serde::Serialize + Deserialize` on data types that cross boundaries (API, storage).
- TOML for config files. JSON for API responses. Cap'n Proto for Veilid protocol (Phase 3).

## Dependencies
- All version pins at workspace root. Crates use `workspace = true`.
- Adding a new dependency requires justification (see SECURITY.md §9.2).
- Prefer existing workspace deps over new ones.
