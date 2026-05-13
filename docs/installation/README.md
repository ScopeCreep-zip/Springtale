# Installation

Springtale ships as a single static daemon (`springtaled`) plus a CLI
(`springtale`). There are four supported ways to install it. Pick whichever
matches how you already run things — they all give you the same daemon.

| Method | Use when |
|---|---|
| [From source](from-source.md) | You want to read or modify the code. Default for contributors. |
| [Docker / Compose](docker.md) | You're running on a server, in a container orchestrator, or you don't want Rust on your dev machine. |
| [Nix](nix.md) | You're already a Nix user. The flake gives you a reproducible dev shell or a deployable derivation. |
| [systemd](systemd.md) / [launchd](macos.md) | You're installing as a long-running daemon on Linux / macOS. |

There are no pre-built binaries yet. The single-static-binary story is
real, we just haven't cut a release. When we do, downloads will appear
on the [GitHub Releases page](https://github.com/ScopeCreep-zip/Springtale/releases).

## Common ground

Whichever method you pick:

- The vault file is written to `~/.local/share/springtale/` by default. It contains your Ed25519 identity, your encrypted connector credentials, and your bot session memory. Back it up; if you lose it, you lose your bot.
- The HTTP API binds `127.0.0.1:8080` by default. **Do not bind `0.0.0.0`** without reading the security guide first — Springtale's API expects to be local-only.
- The first time you run `springtale init`, you set a vault passphrase. There is no recovery flow. Forget the passphrase, lose the vault.
- Springtale makes **zero outbound connections at idle** by design. The only network traffic happens when a configured connector or rule fires.

## Hardware

Springtale's footprint is modest:

| Resource | Idle | Active (10 formations, 5 connectors) |
|---|---|---|
| RAM | ~30 MB | ~150 MB |
| CPU | <1% | ~5% sustained, bursty per tick |
| Disk | ~2 MB binary + vault size (typically <10 MB) | grows linearly with audit retention |

The Tauri desktop adds another ~150 MB (Chromium for the webview, Wasmtime
for any WASM connectors). The web dashboard adds nothing — `springtaled`
serves the SPA directly.

## After installation

→ [`docs/QUICKSTART.md`](../QUICKSTART.md) — first bot in 60 seconds
→ [`docs/guide/first-bot.md`](../guide/first-bot.md) — first useful bot
→ [`docs/operations/`](../operations/) — running it long-term

If something goes wrong, every Springtale error has a stable ID. Run
`springtale fix E001` (or whatever ID appears) for a runbook with
recovery steps. See [`docs/guide/fixing-errors.md`](../guide/fixing-errors.md).
