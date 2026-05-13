# Running as a systemd service

Run `springtaled` as a long-lived daemon on Linux. The unit below is
hardened — drops most kernel capabilities, isolates the filesystem, uses
a dedicated user, and acquires the vault passphrase from a credential file.

## Create the user + data directory

```bash
sudo useradd --system --home /var/lib/springtale --shell /usr/sbin/nologin springtale
sudo mkdir -p /var/lib/springtale
sudo chown springtale:springtale /var/lib/springtale
sudo chmod 700 /var/lib/springtale
```

## Install the binary

Either copy the `target/release/springtaled` you built from source,
install from the Nix package (`nix build .#springtaled` → `./result/bin/springtaled`),
or extract from the Docker image. Land it at `/usr/local/bin/springtaled`:

```bash
sudo install -m 755 target/release/springtaled /usr/local/bin/
sudo install -m 755 target/release/springtale-cli /usr/local/bin/
```

## Store the passphrase

systemd 250+ supports loading credentials from disk into the service's
private credentials directory. Put the passphrase at
`/etc/springtale/passphrase` with tight permissions:

```bash
sudo mkdir -p /etc/springtale
echo -n 'your-long-strong-passphrase' | sudo tee /etc/springtale/passphrase >/dev/null
sudo chmod 400 /etc/springtale/passphrase
sudo chown root:root /etc/springtale/passphrase
```

The unit below loads this via `LoadCredential=` so it lands at
`$CREDENTIALS_DIRECTORY/passphrase` inside the service and is unmapped
from everywhere else.

## The unit file

Save as `/etc/systemd/system/springtaled.service`:

```ini
[Unit]
Description=Springtale automation daemon
Documentation=https://github.com/ScopeCreep-zip/Springtale
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/springtaled
Restart=on-failure
RestartSec=5

User=springtale
Group=springtale
WorkingDirectory=/var/lib/springtale

# Vault passphrase via systemd credential.
LoadCredential=passphrase:/etc/springtale/passphrase
Environment=SPRINGTALE_PASSPHRASE_FILE=%d/passphrase
Environment=SPRINGTALE_DATA_DIR=/var/lib/springtale

# Hardening.
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectKernelLogs=true
ProtectControlGroups=true
ProtectClock=true
ProtectHostname=true
ProtectProc=invisible
PrivateTmp=true
PrivateDevices=true
PrivateUsers=true
PrivateMounts=true
RestrictNamespaces=true
RestrictRealtime=true
RestrictSUIDSGID=true
MemoryDenyWriteExecute=true
LockPersonality=true
RemoveIPC=true
UMask=0077

# Only the data dir is writable.
ReadWritePaths=/var/lib/springtale

# Drop every capability we can.
CapabilityBoundingSet=
AmbientCapabilities=

# Syscall filter — allow what a typical Rust + tokio + rustls program
# needs, deny everything else. Audit with `systemctl status` if a
# legitimate syscall is blocked.
SystemCallFilter=@system-service
SystemCallFilter=~@privileged @resources @mount @swap @reboot @debug
SystemCallArchitectures=native

# Address families — rustls needs INET/INET6; UNIX for the local
# transport socket. Drop everything else.
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6

[Install]
WantedBy=multi-user.target
```

## Enable + start

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now springtaled
sudo systemctl status springtaled
```

Logs go to the journal:

```bash
sudo journalctl -u springtaled -f          # follow live
sudo journalctl -u springtaled --since="1 hour ago"
```

## First-run setup

`springtaled` won't start cleanly until the vault + database exist.
Run `init` once as the springtale user:

```bash
sudo -u springtale SPRINGTALE_PASSPHRASE_FILE=/etc/springtale/passphrase \
  SPRINGTALE_DATA_DIR=/var/lib/springtale \
  springtale-cli init
```

After that, `systemctl restart springtaled` boots cleanly.

## CLI from the host

The CLI talks to the daemon over HTTP. You'll need the auth token —
generated at init time and stored in the data directory:

```bash
sudo -u springtale springtale-cli auth print
```

Add it to your environment:

```bash
export SPRINGTALE_API_TOKEN=<token>
springtale-cli connector list
```

## Removing

```bash
sudo systemctl disable --now springtaled
sudo rm /etc/systemd/system/springtaled.service
sudo systemctl daemon-reload
```

Data lives at `/var/lib/springtale/`. Decide whether to keep it — see
[`docs/operations/backup-and-restore.md`](../operations/backup-and-restore.md)
for the proper way to archive a vault before deleting.

## Common issues

| Symptom | Likely cause | Fix |
|---|---|---|
| `journalctl` shows `Permission denied: /var/lib/springtale` | Wrong ownership on the data dir | `sudo chown -R springtale:springtale /var/lib/springtale` |
| Service exits with `E001: vault not initialized` | First-run `init` not done | Run the `springtale-cli init` step above |
| `Failed to load credential passphrase` | systemd version <250 | Either upgrade or replace `LoadCredential` with `EnvironmentFile=` reading a 0400 file |
| Syscall blocked in journal | Hardening is too strict for some specific connector | Add a permissive line: `SystemCallFilter=@<group>` — file an issue so we can tighten the default |
