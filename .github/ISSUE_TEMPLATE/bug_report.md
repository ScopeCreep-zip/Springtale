---
name: Bug report
about: Something is broken or behaves differently from the docs
title: ''
labels: bug
assignees: ''
---

## What happened

<!-- One or two sentences. What did you see that you didn't expect? -->

## What should have happened

<!-- One or two sentences. What did the docs (or your model of the system) make you expect? -->

## How to reproduce

1. …
2. …
3. …

## Environment

- **Springtale version** (git commit hash or release tag):
- **OS** (uname -a output is fine):
- **Rust toolchain** (`rustc --version`):
- **Run mode**: [ ] cargo run / [ ] release binary / [ ] Docker / [ ] Nix / [ ] Tauri desktop / [ ] web dashboard

## Logs

<!-- The relevant chunk of `springtaled` logs. Redact secrets BEFORE pasting. The vault passphrase, API tokens, and `Secret<T>` fields should never appear in logs, but double-check. -->

```
paste logs here
```

## Sentinel verdict (if applicable)

<!-- If the bug involves an action being denied, paste the audit_trail row for it. Get it via: -->
<!-- sqlite3 ~/.local/share/springtale/springtale.db "SELECT * FROM audit_trail ORDER BY id DESC LIMIT 5;" -->

## Workaround

<!-- If you've already found one, share it. -->

## Privacy / safety note

If reproducing this bug requires sharing any identity-linkable data
(usernames, account IDs, channel names, IPs in logs), redact them
before posting. We don't need real values to reproduce — placeholders
or hashes are fine.

If the bug itself involves a user-safety failure (identity leak, panic
wipe leaving artefacts, duress vault being detectable), close this
issue and report through [`SECURITY.md`](../../SECURITY.md) instead.
