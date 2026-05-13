# Backup and restore

Springtale's state lives in one directory. Back that up, you have
everything. Lose it, you lose your bot identity, your vault, your
formations, your mental models, your audit trail. There is no
remote-stored anything; nobody else has your data.

This is also why the project ships `travel mode` as a first-class
feature: the same flow that backs up the daemon is the flow you use
to move it between devices, e.g. across borders. See the threat model
in [`docs/current-arch/SECURITY.md`](../current-arch/SECURITY.md) §2.7.

## What's in scope

```
$SPRINGTALE_DATA_DIR/
├── vault.bin          ← Ed25519 identity + connector credentials + duress region
├── springtale.db      ← rules, events, formations, mental_model, audit_trail
├── api_token          ← HMAC bearer token (regenerate-able)
├── connectors/        ← installed connector manifests + WASM binaries
└── audit.log          ← rolling audit log (mirrored to DB)
```

Plus the **passphrase** itself, which is *not* in the data directory.
You can back up the data and have nothing useful if you don't remember
the passphrase. Write it down somewhere offline.

## What's NOT in scope

- The daemon binary itself. Rebuild from source.
- The OS-level configuration (systemd unit, launchd plist). Tracked
  separately if you use config management.
- The Tauri desktop preferences (`~/Library/Preferences/run.springtale.*`
  on macOS, `~/.config/springtale-desktop/` on Linux). These are
  cosmetic — window position, last-used theme. Don't bother backing up.

## The right way: `travel prepare`

This is the only flow that's safe with the daemon running and is the
flow the CLI is designed around.

```bash
springtale-cli travel prepare --backup-to /path/to/backup.tar.gz.enc
```

What it does:

1. Pauses all formations (so cooperation state quiesces).
2. Flushes WAL files into the main database.
3. Takes a consistent snapshot of the data directory.
4. Encrypts the snapshot with a key derived from your vault passphrase
   via Argon2id (separate KDF from the vault, fresh salt).
5. Writes the encrypted archive to `--backup-to`.
6. Resumes formations.

The archive format is:

```
backup.tar.gz.enc
├── header (16 bytes magic + 32 bytes salt + 24 bytes nonce + version byte)
└── ciphertext (XChaCha20-Poly1305 of the gzipped tar)
```

You can inspect the header but you can't read the contents without the
passphrase. The Argon2id parameters match the vault's (64 MB, 3 iters,
4 parallelism) so cracking is GPU-resistant.

### Restoring

```bash
springtale-cli travel restore --from /path/to/backup.tar.gz.enc
```

What it does:

1. Verifies the data directory is empty (refuses to overwrite). Move
   the existing directory aside first if you want to start clean.
2. Prompts for the passphrase used at backup time.
3. Decrypts the archive, verifies integrity, writes the directory.
4. Runs schema-apply to bring the database forward if the binary is
   newer than when the backup was taken.

## The other right way: cold copy

If the daemon is stopped:

```bash
systemctl stop springtaled        # or launchctl unload, or docker compose down
tar czf backup.tar.gz -C "$HOME/.local/share" springtale/
gpg --symmetric --cipher-algo AES256 backup.tar.gz
# now backup.tar.gz.gpg is portable + encrypted
rm backup.tar.gz
systemctl start springtaled
```

Restore:

```bash
gpg --decrypt backup.tar.gz.gpg | tar xzf - -C "$HOME/.local/share/"
```

This is slightly faster than `travel prepare` and uses tools you
probably already trust. The trade-off is that you have to coordinate
the stop/start yourself and you can't use it as the migration flow.

## The wrong way: cp while running

Don't. SQLite's WAL means the .db file is **not consistent on disk** at
all times. `cp springtale.db elsewhere` while the daemon is running
will produce a backup that may be unreadable or, worse, readable but
missing recent writes.

If you must do this, use SQLite's online backup API via the CLI:

```bash
springtale-cli data export --output state.json --encrypt
```

This is a logical export (JSON), not a binary copy. Slower and
larger but safe-while-running. Restore via:

```bash
springtale-cli data import --input state.json
```

Note that JSON export doesn't include WASM connector binaries (they're
re-installed from manifest on restore) or the audit log (rotation is a
separate concern). It's for **data**, not **state**.

## Verification

Every backup should be tested before you need it. The cheapest test:

```bash
# On a different machine, in a different data dir:
SPRINGTALE_DATA_DIR=/tmp/springtale-restore-test \
  springtale-cli travel restore --from /path/to/backup.tar.gz.enc
SPRINGTALE_DATA_DIR=/tmp/springtale-restore-test \
  springtale-cli connector list
SPRINGTALE_DATA_DIR=/tmp/springtale-restore-test \
  springtale-cli rule list
rm -rf /tmp/springtale-restore-test
```

If both lists match what's on your live system, the backup is good.

## Backup frequency

Springtale's data changes constantly when bots are running — every
trigger fire writes an event row, every cooperation tick can write
momentum and mental-model rows. But the **important** data changes
rarely:

- Vault contents (identity + connector credentials): when you add a
  connector or change a credential.
- Rule definitions: when you author or edit rules.
- Mental models: continuously, but losing the last N minutes is rarely
  catastrophic.

A sensible cadence:

- **After every significant change to rules or connectors** — manually,
  via `travel prepare`.
- **Daily snapshot** — via cron, while daemon is paused.
- **Continuous backup of the data directory** — via your standard
  backup tool (`restic`, `borg`, `rsnapshot`). The data is encrypted
  at rest already; your backup tool's encryption is defence in depth.

## Disaster recovery sequence

You wake up and the machine is gone (theft, ransomware, hardware
death). You have a backup somewhere. The sequence:

1. New machine. Install Springtale (any method in [installation](../installation/)).
2. `springtale-cli travel restore --from backup.tar.gz.enc`. You'll
   need the passphrase.
3. Restart the daemon. It picks up the restored data directory.
4. **Verify connectors come back online.** Some need re-auth (Telegram
   tokens are stable; OAuth flows like Kick may need a re-pair).
5. **Verify formations resume.** Check `springtale-cli rule list` shows
   your rules and `GET /formations` shows your formations.

Time-to-recovery for a tested backup is typically <5 minutes.

## What an adversary can recover from an old backup

If they have the file but not the passphrase: nothing useful.
XChaCha20-Poly1305 with Argon2id is not crackable on modern hardware
within reasonable time bounds for a strong passphrase.

If they have the file and a weak passphrase: everything in the backup,
which is everything Springtale stored at backup time — connector
credentials, identity keypair, formation history, audit trail.

If they have the file and your real passphrase (compelled disclosure):
they get the **decoy** vault if your passphrase opens that one. See
[`docs/guide/security.md`](../guide/security.md) §7 and the duress
passphrase entry for the dual-region vault.
