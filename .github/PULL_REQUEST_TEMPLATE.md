<!-- One-line summary in the PR title. Body sections below. -->

## What this changes

<!-- What does the diff do? 1-3 sentences. -->

## Why

<!-- Why is this the right change? What problem does it solve? -->
<!-- If it's a refactor, what does it unlock that the previous shape blocked? -->

## How to verify

<!-- Concrete steps a reviewer can run to confirm the change works. -->
<!-- "cargo test passes" is not enough. -->

## Phase / scope

- [ ] This change fits the [current roadmap phase](../docs/ROADMAP.md)
- [ ] This is a single concern (not "fix bug + refactor + new feature" bundled)

## Checks

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo nextest run --workspace` (or `cargo test --workspace`)
- [ ] Frontend changes built with `pnpm build` (if applicable)
- [ ] No new unsafe blocks (or every new one has a `// SAFETY:` comment)
- [ ] No new `unwrap()` / `expect()` / `panic!()` in library crates
- [ ] No new `native-tls` or OpenSSL dependencies — direct or transitive
- [ ] No new telemetry, analytics, or "anonymized usage" reporting
- [ ] All new credentials wrapped in `Secret<T>`
- [ ] Docs updated (if behaviour changed)

## Security review needed?

- [ ] No — pure refactor / test / docs change
- [ ] No — touches non-security paths only
- [ ] Yes — touches `springtale-crypto`, `springtale-connector`, `springtale-sentinel`, or any path that handles secrets / capabilities / signed manifests

If yes, tag a security reviewer in addition to the functional reviewer.

## Anything else

<!-- Trade-offs you made, things you wanted to do but didn't, related work -->
<!-- you're punting to a follow-up issue, design questions for the reviewer. -->
