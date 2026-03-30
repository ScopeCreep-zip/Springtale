---
paths:
  - "**/*.rs"
  - "**/tests/**"
---

# Testing Conventions

## Unit Tests
- Place in same file: `#[cfg(test)] mod tests { ... }`
- Test name pattern: `test_{function}_{scenario}_{expected}`
- Use `assert_eq!` for equality, `assert!(matches!(...))` for enums.
- Each test is independent. No shared mutable state between tests.

## Integration Tests
- Place in `tests/` directory at crate root.
- One file per integration scenario.
- Use `tokio::test` for async tests.

## Test Commands
```bash
cargo nextest run --workspace              # fast parallel test runner (preferred)
cargo test --workspace                     # standard test runner
cargo test -p springtale-core              # single crate
cargo test -p springtale-core -- test_name # single test
cargo test --doc                           # doc tests
```

## What to Test
- All public API surface.
- Error paths (invalid input, network failure, permission denied).
- Boundary conditions (empty collections, max values, zero values).
- Security invariants (capability checks, signature verification, Secret<T> non-exposure).

## What NOT to Test
- Private implementation details that may change.
- Trivial getters/setters.
- Third-party library behavior.

## Mocking
- Use trait objects for dependency injection. No mock frameworks required for most cases.
- For network calls: mock at the client layer, not at reqwest level.
- For storage: use in-memory SQLite (`":memory:"`).
