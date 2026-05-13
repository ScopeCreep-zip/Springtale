# OPSEC

Operational security for running Springtale in adversarial
environments. This page is for users whose threat model includes
targeted attention — activists, journalists, IPV survivors, people
in jurisdictions where the work they do is contested.

If your threat model is "I don't want random data brokers to have
my chat history", you're fine with the defaults. Skip this page.

---

## Layers

Springtale provides:

1. **Encryption at rest** (vault, SQLite, mental model).
2. **Encryption in transit** (rustls everywhere, no plaintext fallback).
3. **Sandbox** for community code (Wasmtime).
4. **Capability allow-lists** (no wildcards, no escalation).
5. **No telemetry** (we make zero outbound connections at idle).
6. **Duress mechanisms** (panic wipe, duress passphrase, disguise tray, quick-hide).

What you provide:

1. **Strong passphrase** for the vault.
2. **Hardware you control** for storage and processing.
3. **OS-level hardening** beyond what Springtale touches.
4. **Network OPSEC** that's compatible with what your connectors do.
5. **Procedural discipline** — backup, key rotation, log review.

The layers compose. If any layer fails, the others limit damage.

---

## Vault passphrase

The single most important variable in your OPSEC. The vault is
unrecoverable; a weak passphrase makes it crackable.

**Minimum recommended:**

- 5+ words from a diceware wordlist (>50 bits of entropy).
- Not derivable from anything an attacker can learn about you.
- Memorised, not typed from a file that lives on the same device.

**Better:**

- 7+ words from a long diceware wordlist.
- Stored offline on paper in two physical locations you trust.
- Never spoken aloud near a smart speaker / phone. The attack
  surface here is real; voice assistants get used as
  surveillance proxies.

**Don't:**

- Use a password manager that itself unlocks with a passphrase you
  remember. You've just moved the problem.
- Reuse the passphrase from another service.
- Email it to yourself "for safekeeping".

---

## Where the vault lives

Default: `~/.local/share/springtale/vault.bin`. On macOS:
`~/Library/Application Support/Springtale/vault.bin`. On Docker
deployments: wherever you bind-mount the data dir.

Considerations:

- **Filesystem encryption.** Springtale encrypts the vault file
  contents; the filesystem may also do an extra layer (LUKS, BitLocker,
  FileVault, APFS+FileVault). Use both. Defence in depth.
- **Hidden volume.** Some users put the data dir on a VeraCrypt
  hidden volume. The duress passphrase mechanism already does this
  at the application layer; whether to add VeraCrypt depends on
  your threat model.
- **External storage.** Mounting the data dir from a USB key /
  external drive is fine. Be aware: the USB key + its passphrase
  are both required to operate the daemon. Losing either is
  losing the bot.

---

## Network OPSEC

When a connector fires, it makes outbound TLS connections. Your
network sees:

- The destination hostname (via SNI).
- The IP address.
- Connection timing.
- Approximate traffic volume.

The contents are encrypted; the metadata is not.

If your threat model includes "the network operator must not see I
talk to Telegram":

- **Use a VPN.** Standard mitigation. Moves trust from your ISP to
  your VPN provider. Pick one whose threat model matches yours.
- **Use Tor.** Higher latency; cooperation tick-rate becomes
  unusable. Acceptable for low-frequency rules (daily cron jobs,
  occasional webhooks).
- **Use a hosted relay over a private mesh.** WireGuard / Tailscale
  to your own bastion that exits via a trusted ISP. Worth setting
  up if you're going to run Springtale long-term in a hostile
  environment.

DNS:

- Resolve through DNS-over-HTTPS / DNS-over-TLS. Configure at OS
  level. Springtale doesn't override the system resolver.

---

## Process hardening

Recommended additions beyond Springtale's defaults:

### Linux (systemd unit)

The [systemd unit](installation/systemd.md) we ship includes:

- `NoNewPrivileges=true`
- `ProtectSystem=strict`, `ProtectHome=true`
- `PrivateTmp=true`, `PrivateDevices=true`, `PrivateUsers=true`
- `CapabilityBoundingSet=` (empty — no capabilities)
- `SystemCallFilter=@system-service` with `~@privileged @resources`
- `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6`
- `MemoryDenyWriteExecute=true`

That's already pretty tight. Additional options if you want more:

- `LockPersonality=true` (we have it)
- `RestrictNamespaces=true` (we have it)
- `IPAddressDeny=any` + `IPAddressAllow=<specific>` — restrict outbound IPs

### macOS

- File full-disk encryption (FileVault, on by default in modern macOS).
- Run under a separate macOS user account, not your daily-driver
  account. Limits attack surface from anything else running as
  that user.
- Disable Spotlight indexing on the data dir:
  ```
  mdutil -i off "$HOME/Library/Application Support/Springtale"
  ```
- Sign the binary or remove the quarantine attribute (see
  [`docs/installation/macos.md`](installation/macos.md)).

### Windows

Not yet a primary supported target — works but less polished.
Recommendations:

- BitLocker on the drive holding the data dir.
- Run under a non-admin Windows user.
- Disable Windows search indexing on the data dir.

---

## Logs and forensic traces

Logs (whether journald, plain file, or Docker driver) record metadata
about every action: rule name, connector, outcome, timestamp. They
don't record action content — but the metadata itself is sensitive.

- **Set tight retention.** `journalctl` defaults can keep weeks of
  logs. Tune `SystemMaxUse=` aggressively.
- **Consider in-memory logging only** for high-sensitivity work:
  pipe stdout to `/dev/null` and rely on the audit trail in the
  encrypted database for after-the-fact review.
- **Audit log retention.** Set `[sentinel] audit_retention_days`
  to a duration you'd actually want recoverable. Default 90 days;
  consider 30 or even 7 for sensitive deployments.

---

## Identity hygiene

The Ed25519 keypair in your vault IS your bot's identity. Anyone who
gets the vault has the identity.

- **Don't reuse identities across threat models.** If you run a
  public stream-alert bot and a private mutual-aid coordination
  bot, give them separate vaults (separate Springtale installs,
  separate data dirs, separate passphrases).
- **Rotate proactively** if you suspect compromise:
  `springtale-cli vault rotate-identity`.
- **Connector credentials are linked to your real-world identity**
  on most platforms (Telegram phone numbers, Discord email,
  Bluesky handle). Springtale can't unlink this; choose accounts
  whose identity model matches your threat model.

---

## Travel mode

When crossing borders or leaving devices behind:

```bash
springtale-cli travel prepare --backup-to backup.tar.gz.enc
springtale-cli panic                                    # wipe locally
# carry backup.tar.gz.enc by whatever means
springtale-cli travel restore --from backup.tar.gz.enc  # at destination
```

The encrypted archive is unreadable without your passphrase. Cloud
storage, USB, sent over Signal — any transport is fine if you trust
your passphrase.

Caveats:

- The Springtale binary stays on the device. Its presence is evidence
  Springtale was used. If that's in scope: remove the binary too.
- Backup metadata (file timestamps, swap pages, filesystem journal
  entries) may retain traces. Full-drive secure-erase if you need
  to defeat forensic analysis.

---

## Disguise tray + quick-hide

For shoulder-surfing / quick-hide scenarios:

- **Disguise tray icon** (`POST /safety/disguise/profile`): pick
  `calculator`, `files`, or `notes`. The tray icon is what an
  observer sees on a taskbar.
- **Quick-hide** (`SafetyConfig.quick_hide_shortcut`, default
  `Ctrl+Shift+H`): OS-wide global hotkey hides the window and
  locks the vault. Works from any application, not just when
  Springtale has focus.
- **Window title**: also configurable. A window titled "Calculator"
  is less of an alert than one titled "Springtale Bot Daemon".

These don't defeat a determined adversary who looks at process lists
or filesystem state. They defeat a glance at your screen.

---

## Recovery

After an incident — confirmed or suspected compromise:

1. **Stop the daemon.** Don't let any in-flight rules keep firing.
2. **Snapshot before changes.** `cp -a ~/.local/share/springtale/
   forensic-snapshot/` to preserve evidence.
3. **Rotate identity.** New vault, new keys, new connector
   credentials.
4. **Re-pair connectors.** Telegram bots, Discord apps — anything
   that pairs to your identity needs re-pairing.
5. **Audit the snapshot.** `springtale-cli data export --output
   incident.json` against the snapshot. Look at recent rules,
   formation history, audit trail.
6. **Tell the people whose state you held.** If your bot moderated
   a community, the community should know about the compromise
   even if no data was leaked, because they reasonably assumed it
   wouldn't be.

---

## What we can't help with

- **Compelled disclosure where the adversary watches you type.**
  Duress passphrases defend against post-hoc forensic analysis,
  not real-time observation.
- **Pre-installed stalkerware that watches the OS.** Springtale's
  encryption doesn't help if a keylogger captures your passphrase
  on entry.
- **Hardware-level attacks.** Cold boot, JTAG, SoC vulnerabilities.
  Out of scope.
- **Social engineering against your humans.** Don't trust that
  the docs the contributor onboarding emails point to are
  unmodified if you can't verify the GitHub repo state.

If any of these are in your threat model, Springtale is one layer
among many. Coordinate with security professionals who specialise
in your specific risk profile.
