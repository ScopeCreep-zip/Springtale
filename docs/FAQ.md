# Frequently Asked Questions

Common questions, honest answers. If your question isn't here and the
docs don't cover it, open a Discussion (not an issue).

## What is Springtale, in one sentence

A local-first automation platform that lets you wire together chat
platforms, APIs, filesystems, and AI models without trusting anything
to phone home or hold your credentials.

## Who is this for

People whose safety depends on privacy. Trans people, POC, activists,
IPV survivors, immigrants, sex workers, journalists. Also: anyone who
wants automation without "free service collects your data".

If you're trying to automate posting cat pictures across three social
networks, Springtale works. If you're trying to coordinate a mutual-aid
network without a single platform seeing the membership graph, that's
the kind of thing Springtale is built for.

## Why not just use Zapier / IFTTT / n8n / Activepieces

Those all either store your credentials on their servers, run in
their cloud, or send telemetry. None of them are an option for someone
whose threat model includes "the automation provider is compelled to
hand over data".

Springtale runs entirely on your device. No accounts, no cloud, no
telemetry. The trade-off is you operate it yourself.

## What does "no telemetry" actually mean

The daemon makes **zero outbound connections at idle**. When a
connector or rule fires, the connector makes its own outbound calls
to its target service (e.g. `connector-telegram` calls `api.telegram.org`).
Springtale itself never calls anything.

There is no analytics SDK. No error reporting. No "anonymized usage"
data. No update check. No version-phone-home. If you `tcpdump` an idle
Springtale, you see nothing.

## Does it work without AI

Yes. The default AI adapter is `NoopAdapter` — it returns "no AI
configured" for any AI call. Rules, automations, command routing,
cooperation, and bot commands all work without AI plugged in.

Plug in Ollama (local), Anthropic, or OpenAI-compatible if you want.
Each adapter is per-bot, swappable at runtime. Unplug it and nothing
breaks.

## Will my ISP / employer / dorm network see I'm using it

At idle, no — the daemon doesn't talk to the network.

When a configured rule fires, the connector for that rule makes an
outbound HTTPS request. Your network sees an HTTPS connection to
whatever service the connector calls (`api.telegram.org`, `discord.com`,
`api.github.com`, etc.). The contents are encrypted; the destination
is visible.

If "my network sees I'm using Telegram" is in your threat model, that
visibility is the same as if you used Telegram in a browser. Springtale
doesn't add new exposure beyond what the underlying service requires.

For deeper threat model — running on adversarial networks, traffic
analysis, Tor compatibility — see [`docs/opsec.md`](opsec.md).

## What happens to my data if my laptop dies

It's gone. Springtale stores everything locally; there is no remote
backup, no cloud, no recovery service. If you don't have a backup,
you don't have the data.

Use `springtale-cli travel prepare --backup-to <path>` regularly. See
[`docs/operations/backup-and-restore.md`](operations/backup-and-restore.md).

## What if I forget my passphrase

The vault is unrecoverable. There is no recovery flow, no support
hotline, no master key. This is by design — anyone who could recover
your vault could also be compelled to do so.

Write your passphrase down somewhere offline. Keep two copies in
different locations. Don't trust your memory.

## Is the duress passphrase really safe

The dual-region vault is constant-size (131,152 bytes) and writing
one region never touches the other byte-for-byte. An adversary who
sees you decrypt your vault cannot tell, from the file alone, whether
you decrypted the primary or the decoy.

Caveats:

- If they're watching your shell or screen, they can see *which
  passphrase you typed*. The duress feature defends against
  forensic analysis of the file, not real-time observation.
- The decoy needs to be plausibly populated. An empty decoy is a
  giveaway. Add some real-looking-but-disposable connectors.
- If they have an old backup taken before you set up the duress
  passphrase, comparing it to a current backup tells them you've
  added a decoy region. Take a fresh backup after duress setup.

See [`docs/current-arch/SECURITY.md`](current-arch/SECURITY.md) §2.6
for the formal threat model.

## Can I run this on a VPS

Yes, but read the security guide first. The VPS provider can see
everything you run, every file on the disk, every network packet.
Springtale's encryption-at-rest defends against opportunistic
snooping (a stolen disk, a misconfigured backup) but not a determined
hypervisor.

If your threat model includes the VPS provider, run Springtale on
hardware you physically control.

## Why Rust

- Memory safety without GC overhead. Important for a daemon doing
  crypto and network I/O.
- The Wasmtime sandbox lives in Rust. So does `rustls`. So does most
  of the ecosystem we care about.
- `Secret<T>` from `secrecy` gives us compile-time + runtime
  guarantees that secrets don't accidentally serialize or log.
- `cargo` makes the dependency graph auditable. `cargo audit` runs
  every CI.

If you'd prefer a Python / Go / TypeScript version, this isn't it.
Springtale's threat model basically requires "no JIT, no garbage
collector pauses around secret zeroing".

## Why not just use MCP servers directly

MCP servers usually run as separate processes per connector. Each one is a
fresh attack surface. The OSS landscape currently has 1,000+ MCP
servers with no signing, no capability declarations, no sandbox —
everything OpenClaw made famous.

Springtale exposes every installed connector through a single MCP
server — `springtale-mcp` over the whole registry, mounted by the
daemon at `/mcp` behind an Origin check and bearer auth, with the same
capability checks and signed manifests as direct dispatch. One process,
not one per connector. Same security posture
whether the caller is your bot, a CLI command, or an external AI
calling via MCP.

If you only need one MCP server and don't care about sandboxing, run
that server directly. If you want N connectors with shared
capability + audit + secret handling, that's where Springtale fits.

## What about MCP-style oauth flows

Connectors that need OAuth (Kick, Discord) handle the flow inside the
connector. The token is stored in the vault. Springtale's HTTP API
exposes connector-setup endpoints that walk the user through the
OAuth dance from the dashboard.

OAuth tokens never leave the daemon's process memory in plaintext
(they're `Secret<String>` and AEAD-encrypted at rest in the vault).

## Why is `connector-matrix` not in the workspace

`matrix-sdk` 0.16 pins `rusqlite` 0.37 with an unpatched CVE
(heap info disclosure, CVE-2025-70873). Springtale uses rusqlite 0.39
which has the patch. Until upstream catches up, including
`connector-matrix` would compromise the rest of the daemon's database
layer.

When `matrix-sdk` updates, we'll ship it.

## How does the cooperation framework relate to bot orchestration

Cooperation is for **multiple agents inside one bot working together**.
The orchestrator inside `springtale-bot` is a separate concern — it
decomposes an intent into sub-tasks when a formation reaches Fever
momentum tier. The orchestrator uses AI; the rest of the cooperation
framework does not require AI.

Read [`docs/guide/cooperation.md`](guide/cooperation.md) for the
deeper model. The seven cooperation deep-dive guides under
`docs/guide/` cover each subsystem in detail.

## When will Phase 3 / Veilid ship

No promise. The transport stub exists; the Veilid project itself is
still maturing. We won't ship a Veilid-mesh release until the upstream
crate is stable enough that we'd trust running real users' identity
through it.

If you want to track progress: [Veilid project](https://veilid.com).

## Can I contribute anonymously

Yes. See [`docs/anonymous-contribution.md`](anonymous-contribution.md).
No CLA, no real-name requirement, no PGP key requirement.

## Where do I report a bug vs a vulnerability

- **Bug** that doesn't put users at risk: open an issue.
- **Vulnerability** or anything that could compromise a user's
  privacy / safety: don't open an issue. Use the GitHub Security
  Advisory flow or email k1104jackson@hotmail.com. See [`SECURITY.md`](../SECURITY.md).

If in doubt, default to security disclosure. We'd rather hear
something twice than miss it once.

## How do I support the project

- Use it, file bugs, contribute fixes.
- Spread the word among people who'd benefit from privacy-respecting
  automation.
- Sponsor: see [`.github/FUNDING.yml`](../.github/FUNDING.yml).
- Audit the code. Especially `springtale-crypto`, `springtale-connector`,
  and `springtale-sentinel`. Independent eyes on the security paths
  are the most valuable contribution we can receive.

There is no "buy our enterprise version" path. There is no enterprise
version.
