# Log rotation

Springtale writes structured JSON logs to stdout. What happens after
that depends on how you run it.

## Run modes and where logs go

| Run mode | Where logs land | Rotation |
|---|---|---|
| `cargo run` / interactive | Terminal stdout | None — your terminal is your buffer |
| systemd (Linux) | journald | Configured via `journald.conf`'s `SystemMaxUse` / `SystemKeepFree` |
| launchd (macOS) | The path you set in `StandardOutPath` / `StandardErrorPath` in the plist | None — set up `newsyslog` or rotate manually |
| Docker Compose | The Docker logging driver (default: `json-file`) | Driver-specific — set `max-size` and `max-file` in compose |
| Tauri desktop | Inside the desktop app's data dir, plus the OS-native log surface | Tauri rotates per session |

## Log volume

Idle: a handful of lines per hour (heartbeat, dead-man tick).

Active: scales linearly with action dispatches. A bot handling 1
message/second writes ~10–20 log lines/second across rule eval +
sentinel verdict + dispatch + audit.

A long-running daemon writes 100–500 MB of logs per week under
moderate load. Multiply by your audit-retention setting and your trigger
volume.

## Setting log level

```
RUST_LOG=info springtaled                                  # default
RUST_LOG=debug springtaled                                 # everything
RUST_LOG=info,springtale_bot=debug springtaled             # per-crate
RUST_LOG=info,springtale_sentinel=trace springtaled        # sentinel only
RUST_LOG=warn springtaled                                  # quiet mode
```

The format is the same as [`tracing-subscriber::EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html).
We don't recommend `trace` in production — log volume becomes
unmanageable and sentinel verdicts go several lines deep.

## What's in a log line

```json
{
  "timestamp": "2026-05-10T23:14:07.123Z",
  "level": "INFO",
  "target": "springtale_bot::runtime::tick_steps::update_momentum",
  "fields": {
    "formation_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
    "tier": "Hot",
    "successes": 9
  },
  "message": "momentum tier transition"
}
```

Key conventions:

- `formation_id`, `agent_id`, `connector_name`, `rule_id` appear as
  structured fields, not interpolated in the message.
- Secrets are never logged. If you see anything that looks like a
  token, key, or passphrase in a log line, that's a bug — please
  file an issue.
- User-facing strings (chat message content, file paths) are logged
  at `debug` and above, not `info`. Production should not log them.

## systemd / journald

Default config keeps a few weeks of logs and caps total size at ~10%
of disk. Tune in `/etc/systemd/journald.conf`:

```ini
[Journal]
SystemMaxUse=500M
SystemKeepFree=2G
SystemMaxFileSize=50M
SystemMaxFiles=10
```

After editing: `sudo systemctl restart systemd-journald`.

Querying:

```bash
journalctl -u springtaled --since "1 hour ago"
journalctl -u springtaled -p err
journalctl -u springtaled -o json | jq .   # parse the structured fields
```

## launchd / macOS

Add a `newsyslog` stanza at `/etc/newsyslog.d/springtale.conf`:

```
# logfilename                                              [owner:group]  mode  count  size  when  flags  [/pid_file]  [sig_num]
/Users/YOU/Library/Logs/springtaled.out.log                YOU:staff      640   7      *     @T00  Z
/Users/YOU/Library/Logs/springtaled.err.log                YOU:staff      640   7      *     @T00  Z
```

That rotates daily at midnight, keeps 7 archives, gzips old ones.
`newsyslog` runs automatically.

## Docker

In `docker-compose.yml`:

```yaml
services:
  springtaled:
    logging:
      driver: json-file
      options:
        max-size: "100m"
        max-file: "5"
```

Or switch to the journald driver if you're piping into the host's
systemd. Or syslog if you have an aggregator.

## Manual rotation (any path-based log)

If `springtaled` is writing to a file directly and you've got no
external rotator:

```bash
# In a cron / launchd timer:
mv /var/log/springtaled.log /var/log/springtaled.log.$(date +%Y%m%d)
kill -HUP $(pidof springtaled)         # tell it to reopen the file
gzip /var/log/springtaled.log.*        # eventually
```

`springtaled` handles `SIGHUP` by reopening its stdout/stderr targets,
so the rename-then-HUP pattern works without dropping log lines.

## Forwarding logs to an aggregator

If you're already running Loki, Vector, Fluent Bit, or similar:

- **journald source** — read directly from journal on Linux systemd
  installs. Zero config beyond the source.
- **stdout source** — for Docker / Kubernetes, the container runtime
  surfaces logs in your normal pipeline.
- **`tracing-opentelemetry` exporter** — not yet wired into `springtaled`.
  Tracked for a future release. If you need OTLP today, run a sidecar
  that parses the stdout JSON.

## What NOT to put in logs

The daemon should never log:

- Vault passphrases or any KDF output.
- API tokens or HMAC keys.
- Connector credentials (anything wrapped in `Secret<T>`).
- Webhook signature headers.
- Mental-model contents (separate retention policy from operational logs).

If something in this list appears in your logs, it's a bug. File it
under [`SECURITY.md`](../../SECURITY.md), not as a regular issue.

## Log retention vs audit retention

These are two different things:

- **Log retention** — what `journald` / `newsyslog` / Docker do.
  Operational, transient. Lose it, you lose debugging fidelity.
- **Audit retention** — the `audit_trail` SQLite table written by
  `springtale-sentinel`. Persistent, structured, queryable. Lose it,
  you lose the forensic record of every action the daemon dispatched.

Audit retention is controlled by `[sentinel] audit_retention_days` in
your config (default 90 days, can be `0` for forever). The retention
purge runs as a background task; rows older than the threshold are
deleted at the next scheduled run.
