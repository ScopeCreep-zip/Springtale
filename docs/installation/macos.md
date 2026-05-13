# Running as a launchd service on macOS

The equivalent of systemd on macOS. This sets up `springtaled` as a
LaunchAgent (per-user, runs when you log in) or LaunchDaemon (system,
runs at boot).

A **LaunchAgent** is usually what you want for Springtale — the daemon
holds a vault tied to your user identity. A LaunchDaemon makes sense
only if you've explicitly set up a service user and want the daemon
running independent of any logged-in session.

## Install the binaries

```bash
cp target/release/springtaled /usr/local/bin/
cp target/release/springtale-cli /usr/local/bin/
```

You may need `sudo` for `/usr/local/bin` writes on recent macOS — or
install into `~/bin` and adjust the plist's `ProgramArguments` to match.

## Store the passphrase

Use the macOS Keychain:

```bash
# Create the keychain entry — prompts for the passphrase.
security add-generic-password \
  -a "$USER" \
  -s "springtale-vault" \
  -w
```

Then create a tiny wrapper that exports the passphrase from Keychain
and execs `springtaled`:

```bash
cat > /usr/local/bin/springtaled-launch <<'SH'
#!/bin/bash
set -euo pipefail
SPRINGTALE_PASSPHRASE="$(security find-generic-password -a "$USER" -s "springtale-vault" -w)"
export SPRINGTALE_PASSPHRASE
unset SPRINGTALE_PASSPHRASE_FILE
exec /usr/local/bin/springtaled
SH
chmod +x /usr/local/bin/springtaled-launch
```

`security find-generic-password -w` prints the password to stdout; the
wrapper captures it into a process-private env var that launchd's
LSE export doesn't see. The exec'd `springtaled` reads
`SPRINGTALE_PASSPHRASE` and overwrites the in-process variable with a
`Secret<String>` immediately.

## The plist (LaunchAgent)

Save as `~/Library/LaunchAgents/run.springtale.daemon.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>run.springtale.daemon</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/springtaled-launch</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>

    <key>WorkingDirectory</key>
    <string>/Users/REPLACE_ME/Library/Application Support/Springtale</string>

    <key>EnvironmentVariables</key>
    <dict>
        <key>SPRINGTALE_DATA_DIR</key>
        <string>/Users/REPLACE_ME/Library/Application Support/Springtale</string>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>

    <key>StandardOutPath</key>
    <string>/Users/REPLACE_ME/Library/Logs/springtaled.out.log</string>
    <key>StandardErrorPath</key>
    <string>/Users/REPLACE_ME/Library/Logs/springtaled.err.log</string>

    <key>ProcessType</key>
    <string>Background</string>

    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
</dict>
</plist>
```

Replace every `REPLACE_ME` with your actual macOS username. Then:

```bash
mkdir -p "$HOME/Library/Application Support/Springtale"
chmod 700 "$HOME/Library/Application Support/Springtale"
mkdir -p "$HOME/Library/Logs"

launchctl load ~/Library/LaunchAgents/run.springtale.daemon.plist
launchctl start run.springtale.daemon
```

Check it's running:

```bash
launchctl list | grep springtale
tail -f ~/Library/Logs/springtaled.out.log
```

## First-run init

The daemon won't boot cleanly until the vault is set up. Run init
**before** loading the LaunchAgent (or stop the agent, run init,
restart):

```bash
launchctl unload ~/Library/LaunchAgents/run.springtale.daemon.plist
SPRINGTALE_PASSPHRASE="$(security find-generic-password -a "$USER" -s "springtale-vault" -w)" \
  SPRINGTALE_DATA_DIR="$HOME/Library/Application Support/Springtale" \
  springtale-cli init
launchctl load ~/Library/LaunchAgents/run.springtale.daemon.plist
```

## TCC permissions

If you wire connectors that need filesystem access (e.g.
`connector-filesystem` watching `~/Documents`), macOS may prompt for
TCC permission the first time. The first prompt is silent if launchd
spawned the process — check System Preferences → Privacy & Security
for blocked requests and grant manually.

For `connector-shell` invocations, the shell command runs under the
LaunchAgent's process tree, which inherits the granted TCC scope.

## Logs + rotation

Log files at `~/Library/Logs/springtaled.{out,err}.log`. macOS doesn't
rotate by default. Either:

- Use `newsyslog(8)` — add a stanza to `/etc/newsyslog.d/springtale.conf`.
- Let `springtaled` rotate internally (planned, not yet shipped).
- Periodically `mv` and `kill -HUP` from a cron / launchd timer.

See [`docs/operations/log-rotation.md`](../operations/log-rotation.md).

## Removing

```bash
launchctl unload ~/Library/LaunchAgents/run.springtale.daemon.plist
rm ~/Library/LaunchAgents/run.springtale.daemon.plist
security delete-generic-password -a "$USER" -s "springtale-vault"
```

The data directory `~/Library/Application Support/Springtale/` stays
unless you remove it. Decide whether to archive first.

## Threat model notes

- The Keychain entry is tied to your macOS user. Anyone who can unlock your Mac with your password can read it.
- macOS Spotlight indexes `~/Library/Application Support/` by default. The vault is encrypted at rest, but file *names* and timestamps will appear in Spotlight metadata. Consider `mdutil -i off "$HOME/Library/Application Support/Springtale"` to disable indexing for that path.
- LaunchAgents trigger Gatekeeper checks on signed binaries. Building from source produces an unsigned binary; you'll see a quarantine prompt the first time. Either codesign it (`codesign --sign - target/release/springtaled`) or remove the quarantine attribute (`xattr -d com.apple.quarantine target/release/springtaled`).
