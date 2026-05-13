# Threat model FAQ

Honest answers to "is Springtale safe against X?" questions from the
target user perspective. This is not the formal threat model — that's
in [`docs/arch/SECURITY.md`](arch/SECURITY.md) and
[`docs/current-arch/SECURITY.md`](current-arch/SECURITY.md). This is
the plain-language version.

If your specific concern isn't here, open a Discussion. We add
entries here based on real questions; the page only covers what
people actually ask.

---

## Network observability

### Will my ISP / VPN / WiFi operator see I'm using Springtale?

At idle, no. The daemon makes zero outbound network connections when
nothing is happening.

When a configured rule or connector fires, the connector for that
rule makes outbound HTTPS calls to its target service
(`api.telegram.org`, `discord.com`, `api.github.com`, …). Your
network sees TLS connections to those hostnames. The contents are
encrypted; the destination + connection timing aren't.

If your threat model includes "the network operator must not see I
talk to Telegram", Springtale doesn't help. The underlying service
has the same exposure whether you reach it via Springtale or via a
phone app.

### What about DNS leaks?

We use the system resolver by default. For each connector hostname,
the OS resolves it (potentially via the operator-controlled
recursive resolver). If you want DNS-over-HTTPS or DNS-over-TLS,
configure it at the OS or network level — Springtale doesn't
override.

A future hardening item is shipping built-in DoH with a pinned
resolver. Tracked in [AUDIT-NOTES](arch/AUDIT-NOTES.md).

### What about idle keepalives or DHT bootstrapping?

None. The Phase 3 P2P transport (`VeilidTransport`) is currently a
stub — no DHT bootstrap, no peer discovery, no traffic of any kind.
When Phase 3 ships, it will be **opt-in**. You'll decide whether to
go on the mesh.

### Does the daemon make any connections I didn't configure?

No. There is no auto-update check, no version-phone-home, no error
reporting, no analytics SDK, no third-party telemetry. If you
`tcpdump` an idle Springtale, you see nothing.

We commit to this. It's not just "the current implementation is
this way" — it's an architectural promise. See
[ADR 0010](adr/0010-default-deny-approval-gate.md) and the
versioning policy: there is no path to add telemetry. A PR
proposing one would be closed with prejudice.

---

## Device state

### What happens if my device is physically seized?

Depends on what the seizing party can do:

- **Powered off, no passphrase**: vault + database are at-rest
  encrypted (XChaCha20-Poly1305 + Argon2id KDF for the vault, same
  for the SQLite encryption layer). A multi-billion-parameter
  cracking apparatus could brute-force a weak passphrase. A
  diceware-strong passphrase (5+ words from a long wordlist) is
  not crackable on present hardware within reasonable time bounds.
- **Powered on, screen locked**: same as above. The daemon process
  holds keys in memory. A cold-boot attack against the running
  daemon may extract them; this is the threat the `mlock` and
  `zeroize` layers exist to mitigate.
- **Powered on, screen unlocked, you log in**: everything they
  see, they see. No defence here. If you're being compelled to log
  in, your defence is the duress passphrase.

### What's the duress passphrase actually protecting against?

Two scenarios:

1. **Compelled disclosure.** You're being forced to give up a
   passphrase. The duress passphrase opens a decoy vault that
   contains a believable but disposable set of connectors and
   data. The adversary can't tell from the vault file alone
   whether what they got is your real vault or the decoy.
2. **Forensic analysis on a quiescent device.** Same protection.
   The vault file is constant-size (131,152 bytes) and writing one
   region never touches the other; an old backup compared against
   a new one can reveal whether you've used the duress region, but
   the contents aren't inferable.

Limitations:

- If they're observing you in real time (camera, keylogger), the
  duress passphrase is just another passphrase you typed.
- If they have a backup taken before you set up the duress passphrase
  and another taken after, comparison reveals the dual-region
  layout existence. Take a fresh backup post-setup.
- The decoy needs to be plausible. An empty decoy is a giveaway.

### What's the panic wipe actually doing?

`springtale-cli panic` (or entering the duress passphrase, depending
on how you've configured it) triggers a single-pass random overwrite
of the vault key material, followed by deletion of the vault file.
Completes in <3 seconds on a 1 MB vault.

What this DOES protect against:

- Forensic recovery of the vault contents from the SSD/disk after
  the fact. The key material is overwritten before the file is
  unlinked, so even if the deleted-but-not-overwritten file is
  recoverable, the keys are gone.

What this does NOT protect against:

- **Wear-leveling on SSDs.** Modern SSDs may retain prior versions
  of overwritten data internally. Full-drive secure-erase (or
  destruction) is the only deterministic way to defeat this.
- **Filesystem journaling.** Ext4 / APFS / NTFS journals may retain
  metadata and small file fragments after the wipe.
- **Swap / hibernation.** If keys were paged to swap, panic doesn't
  overwrite the swap file. mlock prevents this at the daemon level
  but isn't perfect.
- **Memory dumps.** If a process memory image was captured before
  panic ran, panic doesn't help.

See [`docs/opsec.md`](opsec.md) for the layered hardening to
mitigate these.

### Does the daemon log my data?

The daemon writes structured logs to stdout (then to journald /
file / Docker logs, depending on how you run it). What's in the
logs is **operational** — rule names, connector names, formation
IDs, success/failure outcomes. What's NOT in the logs is **user
content** — chat message bodies, file contents, AI prompts, vault
contents.

The audit trail (a separate SQLite table) records every dispatched
action with timestamp, connector, action name, verdict, and
*action-summary*. The summary deliberately doesn't include payload
content. Audit retention is configurable.

If you see message content, file contents, or anything looking
like a secret in your logs, that's a bug. Report through
[`SECURITY.md`](../SECURITY.md), not as a regular issue.

---

## Connector trust

### How do I know a community connector isn't malicious?

Three layers:

1. **Signed manifest.** Every community connector ships with an
   Ed25519-signed manifest. The signature is verified at install
   time and every load. A tampered manifest fails verification.
2. **Capability allow-list.** The manifest declares exactly which
   capabilities the connector needs (specific outbound hosts,
   specific filesystem paths, etc.). The daemon enforces this at
   dispatch time. A connector with `NetworkOutbound` for
   `api.benign.com` cannot reach `evil.com`.
3. **WASM sandbox.** Community connectors run in Wasmtime with
   fuel metering (10M instructions per invocation), memory limit
   (64 MB), and wall-clock timeout (30s). The host functions are
   the only way out — and host functions are capability-gated.

In addition, the sentinel layer detects **toxic pairs** at install
time (capability combinations that are safe individually but
dangerous together). Manifests declaring `ShellExec +
NetworkOutbound` are rejected.

### Are first-party connectors as safe?

First-party connectors are native Rust, loaded in-process. They get
the same capability gate at dispatch but they're not WASM-sandboxed
— they have full process access if they wanted it.

The trust here comes from **code review** — every first-party
connector is in the workspace, audited, and bound by the same
`forbid(unsafe_code)` / `Secret<T>` / clippy discipline as the rest
of the codebase. Their CI passes the same security lints.

If you don't trust first-party code review, you can build a WASM
version of any first-party connector using the `connector-sdk` —
runs it in the sandbox like a community connector.

### What if a connector author's signing key is compromised?

They generate a new key, re-sign their manifests, publish the new
public key, and revoke the old one. Your daemon won't load the
compromised connector after you've revoked the key.

The current revocation flow is manual (remove from
`trusted_authors.toml`). A formal revocation protocol over the
DHT is a Phase 3 item.

---

## AI adapter trust

### When I plug in an AI adapter, what does it see?

The AI adapter sees the prompt the rule constructed (which may
include user-supplied content via `${trigger.text}` interpolation)
plus any tool definitions you've enabled for that invocation. It
does NOT see:

- The vault contents.
- Other connectors' credentials.
- Other rules' state.
- The Springtale identity keypair.

The AI sees what it needs to do its job: the prompt + the tool list.
If you wire a tool that returns secret content, the AI sees that
content while it's processing — that's a per-rule consideration.

### What if the AI provider is compromised?

Anthropic, OpenAI, etc. can see every prompt you send them. That's
the trust trade-off of using a hosted AI. Three mitigations:

1. **Don't send sensitive content to a hosted AI.** Springtale has
   a built-in two-layer prompt sanitiser (closed `AiRequest` enum
   for compile-time safety + OWASP-pattern runtime scanner) that
   flags credentials, PII, prompt injection patterns. Errors-out
   the request rather than sending.
2. **Use a local model.** `OllamaAdapter` runs against a local
   Ollama process. The model + prompts never leave your machine.
   The `NoopAdapter` (default) literally does nothing — useful
   for testing without an AI.
3. **Use the WASM sandbox for the adapter.** Not yet shipped. A
   WASM-isolated adapter could give third-party AI integrations
   the same trust posture as community connectors.

### Can the AI ignore my safety rules?

The AI can ask. It can't act outside the sandbox. AI-generated
tool calls go through the same capability gate, sentinel verdicts,
and approval gate as any other dispatched action. A prompt-injected
AI cannot escalate beyond what the configured tools + capabilities
allow.

---

## Memory + mental model

### What's in the mental model? Is it sensitive?

The mental model records observations a formation makes about its
own work: domain knowledge confirmed, cooperation patterns that
succeeded, vocabulary the agents converged on, conventions learned.

It DOES NOT contain user content directly. It contains the
*formation's interpretation* of work — "researcher's confidence
threshold > 0.7 yields good writeups" rather than the writeups
themselves.

That said, the mental model can leak inference about user content
(e.g. "this formation handles a lot of Telegram messages about X").
Set `[cooperation] persist_mental_model = false` if your formation
processes data the formation users wouldn't want stored.

### What's in the bot session memory?

Per-user, per-channel conversation history. For chat bots, this
includes the user's messages and the bot's responses. The session
memory is bounded (`[bot] context_window`, default 50 turns) and
AEAD-encrypted at rest in the `bot_memory` table.

Same trade-off as any chat bot: the messages are persisted because
context matters; if your bot handles sensitive conversations, you
need to think about retention. The `[bot] memory_retention_days`
config bounds it.

---

## Operational concerns

### What if I lose my passphrase?

The vault is unrecoverable. There is no recovery flow, no master
key, no support escalation. Anyone who could recover your vault
could be compelled to do so.

Write your passphrase down somewhere offline. Keep two copies in
different locations. Don't trust your memory.

### What if I get locked out (forgot passphrase, lost device)?

Same as above. The data is gone unless you had a backup with the
passphrase.

### What if I want to share access with a co-maintainer?

Currently: share the passphrase. There's no fine-grained access
control inside one daemon. (Multi-user with role-based access is a
post-1.0 consideration, low priority.)

If you both should run the same bot from different machines, copy
the vault between them. See [`docs/operations/host-migration.md`](operations/host-migration.md).
You both have full access to the bot's identity and secrets.

### What if a co-maintainer goes rogue?

Rotate. `springtale-cli vault rotate-identity` + reset every
connector's credentials. The old vault still decrypts with the
old passphrase but has the now-revoked keys.

---

## Specific deployment scenarios

### Can I run this through Tor?

The daemon doesn't natively integrate Tor. Two paths:

- **System-level torify**: `torsocks springtaled` routes the
  daemon's traffic through a SOCKS proxy. Works; latency makes
  cooperation tick-rate unusable.
- **Per-connector Tor**: the planned approach. Specific connectors
  get a configurable SOCKS endpoint. Not yet implemented; tracked.

### Can I run this on Tails?

Should work — Springtale is a static binary with no system
integration beyond `/tmp`, the vault dir, and stdout. On Tails the
vault dir is `~/Persistent/springtale/` if you have persistence
enabled. Test before relying on it.

### Can I run this on a stalkerware-infected device?

No. The threat model assumes the OS is not adversarial. If your OS
is compromised, Springtale's defences don't apply — anything you
type is keyloggable, anything in memory is dumpable, anything on
disk is exfiltrable.

If you suspect stalkerware, the priority is establishing a clean
device, not running Springtale on the infected one. See
[`docs/opsec.md`](opsec.md) for partial-mitigation guidance.

### Can I run this on someone else's hardware (VPS)?

You can, but read [`docs/guide/security.md`](guide/security.md)
first. The VPS provider can see everything: every file on the
disk, every network packet, every process. Springtale's encryption-
at-rest defends against opportunistic snooping; it doesn't defend
against a determined hypervisor.

If your threat model includes the VPS provider, run on hardware
you physically control.

---

## What's NOT in scope

Things Springtale does not try to defend against:

- **Compromised OS kernel.** Out of scope. See above.
- **Hardware attacks** (cold boot, side-channel, evil maid). The
  vault encryption uses standard primitives; we don't claim
  resistance to FBI-budget physical attacks.
- **TEMPEST.** Out of scope.
- **Compulsion of a logged-in user.** The duress passphrase is the
  partial mitigation. Real-time observation defeats it.
- **Network operator who is also adversary AND has root on your
  device.** Game's over.

If your threat model includes any of the above, Springtale is
helpful as one layer among many — not as a complete solution.
