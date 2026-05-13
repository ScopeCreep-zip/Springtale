# Contributing to Springtale

Springtale is built by and for people whose safety depends on privacy.
That shapes everything about how we accept contributions — what code
runs in the daemon, how secrets are handled, what your commits look
like, and how you can show up.

Full contributor reference: **[docs/contributing/](docs/contributing/)**.
This file is the short version, plus the things GitHub surfaces to
first-time contributors before they read anything else.

## Before you write code

- **Read the product model** — [`.claude/rules/shared/product-model.md`](.claude/rules/shared/product-model.md). Bots are the primary unit, AI is a socket, settings are scoped. If a PR doesn't match the model, we'll ask you to reshape it before review.
- **Read the security rules** — [`.claude/rules/backend/security.md`](.claude/rules/backend/security.md). Non-negotiable: `Secret<T>` everywhere, `rustls-tls` only, `#![forbid(unsafe_code)]` on every library crate except `springtale-crypto` and `springtale-connector`, capability-checked dispatch.
- **Read the crate rules** — [`.claude/rules/backend/crate-structure.md`](.claude/rules/backend/crate-structure.md). Modules over inline. `lib.rs` is a table of contents, not a code file.
- **Pick a phase-appropriate task** — [`docs/ROADMAP.md`](docs/ROADMAP.md). Don't open a Veilid-mesh PR while we're still finishing Phase 2b.

## Workflow

1. **Fork + branch.** `main` is the integration branch. Branch off it; never push directly to `main` on the upstream repo.
2. **Build and test before pushing.** At minimum:
   ```bash
   cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo nextest run --workspace
   ```
   Frontend changes additionally:
   ```bash
   cd tauri && pnpm build
   ```
3. **One concern per PR.** A formation refactor + a connector fix + a doc tweak should be three PRs. Reviewers can hold each on its own merits.
4. **Write the commit message for someone reading the log a year from now.** Not "fix bug" — what bug, what symptom, what fix. Examples: `git log --oneline` on `main`.
5. **No `--no-verify` on commits.** Pre-commit hooks are part of the security posture.

## Code review

- Two-track review for security-sensitive paths (`springtale-crypto`, `springtale-connector`, `springtale-sentinel`, anything touching `Secret<T>`, anything writing to the audit trail): one functional reviewer + one security reviewer.
- One-track for everything else.
- "LGTM" is not a review. Either you read the diff and can describe what it does, or you didn't review it.

## Commit signing

We do **not** require GPG-signed commits — the threat model includes
contributors who can't tie a real identity to a key. We do require:

- Honest commit messages (no "Co-Authored-By" trailers for AI tooling unless you actually pair-programmed with that AI and want to claim it).
- DCO sign-off (`git commit -s`) — that's all. No CLA.

## Anonymous contribution

You can contribute under a pseudonym. Use a throwaway email, a
pseudonymous GitHub account, no PGP key. Code is judged on the diff,
not the author. See [`docs/anonymous-contribution.md`](docs/anonymous-contribution.md) for OPSEC notes if your threat model includes attribution risk.

## What we won't merge

- New dependencies on `native-tls` or OpenSSL. Both are banned at the workspace level (`deny.toml` + a vendor stub). We use `rustls` exclusively.
- New `unsafe` blocks without a `// SAFETY:` comment explaining the invariant.
- New telemetry, analytics, or "anonymized usage" reporting. Of any kind. There is no path to add this.
- Connectors that require unofficial APIs likely to get users banned. The threat model includes deplatforming.
- PRs that "fix" code by deleting tests or relaxing clippy lints.

## Where to ask questions

- **Architectural questions** — open a discussion before writing code. Saves both of us time.
- **Found a bug?** Open an issue using the bug-report template.
- **Found a vulnerability?** Don't open an issue. See [`SECURITY.md`](SECURITY.md).
- **Stuck implementing something?** The fastest path is usually to read [`docs/contributing/adding-a-connector.md`](docs/contributing/adding-a-connector.md) and the most similar existing module.

## Code of Conduct

By participating, you agree to [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).
The short version: behave like the project is being read by the people
it's built for. Because it is.
