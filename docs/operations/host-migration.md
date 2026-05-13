# Host migration

Moving a Springtale install from one machine to another. The mechanics
are the same as backup-and-restore, but the use case is different:
you're not protecting against loss, you're consciously moving
infrastructure.

Common scenarios:

- New laptop, retiring the old one.
- Moving the daemon from a workstation to a home server.
- Travel mode — preparing a portable backup, wiping the device, restoring at the destination.
- Disaster planning — keeping a warm spare on a different machine.

## Identity preservation

The Ed25519 keypair in your vault is your bot's identity. If you copy
the vault, the new host **is** the same bot — every signed manifest
verifies, every API token continues working, every paired user still
trusts the new daemon.

If you instead `springtale-cli init` on the new host without copying
the vault, you get a new keypair → a new identity. Paired Telegram
users will see a "different" bot; manifests you signed previously
verify but don't match your new identity.

99% of the time you want **same identity** — preserve the vault.

## Same-identity migration

```
┌─────────────────┐                        ┌─────────────────┐
│   Old host      │                        │   New host      │
│                 │                        │                 │
│ Springtale 0.x  │   1. travel prepare    │  ⌛ idle          │
│ running         │ ────backup.tar.gz.enc─►│                 │
│                 │                        │                 │
│ ⌛ paused        │   2. stop / archive    │  install daemon │
│                 │                        │                 │
│ wipe (optional) │   3. travel restore    │  Springtale 0.x │
│                 │                        │  running        │
└─────────────────┘                        └─────────────────┘
```

Step-by-step:

```bash
# === ON THE OLD HOST ===
springtale-cli travel prepare --backup-to /tmp/migration.tar.gz.enc

# Move the file to the new host. Encrypted, so any transport works:
scp /tmp/migration.tar.gz.enc newhost:/tmp/
# or USB, or matrix self-DM, or whatever. The contents are unreadable
# without the passphrase.

# Optionally wipe the old host — see travel mode below.

# === ON THE NEW HOST ===
# Install Springtale at the same major version.
# (see docs/installation/)

# Restore the backup. Prompts for the passphrase used at backup time.
springtale-cli travel restore --from /tmp/migration.tar.gz.enc

# Start the daemon.
systemctl start springtaled         # or your equivalent

# Verify.
springtale-cli doctor
springtale-cli connector list
springtale-cli rule list
```

The new daemon comes up with the same identity, same connectors, same
rules, same formations, same mental models.

## What about long-lived network state?

A few things don't survive host migration cleanly:

- **OAuth refresh tokens** that bind to the old host's IP. Most don't.
  Kick's tokens are portable; check the individual connector's docs.
- **Telegram webhook URLs.** If you registered a webhook pointing at
  the old host, you need to re-register pointing at the new one. Use
  `POST /webhook/telegram/setup` on the new daemon.
- **Active sessions on chat platforms.** Discord's gateway will
  reconnect transparently. IRC needs to rejoin channels. Telegram
  polling resumes from the last `update_id`.
- **Reverse-proxy state.** Nginx / Caddy / Cloudflare configs need to
  point at the new host.

`springtale-cli doctor` flags connectors that may need re-auth.

## Travel mode

The migration flow doubles as travel mode for adversarial environments
(crossing borders, leaving a device that may be compromised on
return). The cycle:

1. **Prepare** — `springtale-cli travel prepare --backup-to <path>`.
   Encrypted archive of the whole state.
2. **Wipe** — `springtale-cli panic`. Random-overwrite the vault,
   delete the database. <3 seconds on a 1 MB vault.
3. **Travel** — carry the encrypted archive however you like.
   Cloud storage, USB, hidden volume. The contents are inaccessible
   without the passphrase.
4. **Restore** — at the destination, `springtale-cli travel restore --from <path>`.

Between step 2 and 4, there is **no evidence on the device** that
Springtale ran there. Not in `~/.local/share`, not in shell history (if
you used a passphrase file), not in plaintext logs (logs are
truncated by panic).

Caveats:

- The Springtale binary itself stays on the device. Its presence is
  evidence that you *could* have used it. If that's in your threat
  model, also remove the binary.
- Backup metadata on the device (file timestamps, filesystem journal
  entries on ext4/APFS, swap-file pages) may retain forensic traces.
  See [`docs/opsec.md`](../opsec.md) for the deeper threat model on
  device-level forensics.

## Identity rotation

Sometimes you *want* a new identity — e.g. if you believe the old one
is compromised. The flow is:

1. Note which connectors are paired to the old identity (CLI: `connector list --paired`).
2. `springtale-cli vault rotate-identity` — generates new Ed25519
   keypair, re-signs anything that was signed with the old key.
3. Re-pair connectors that bind to the keypair.
4. Re-publish trusted-author keys if you'd previously published one
   under the old identity.

This is destructive — you can't un-rotate. Take a backup first.

## Multi-host fleets

Springtale is single-host by design (at least until Phase 3 / Veilid).
If you want to run a fleet:

- **Same identity, different machines** — copy the vault. The two
  hosts now claim to be the same bot. Sequence reads/writes carefully,
  or they'll fight over webhook subscriptions and OAuth refresh.
- **Different identities, coordinated rules** — each host has its own
  identity. Coordinate via shared state outside Springtale (a queue,
  a shared database table, a webhook fan-out).

There is no official multi-master mode and there won't be one until
Phase 3. The cooperation framework is for multi-agent coordination
*inside one daemon*, not across daemons. (Cross-daemon coordination is
a Veilid milestone.)
