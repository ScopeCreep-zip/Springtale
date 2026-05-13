# Recipe: Scheduled tasks

Run something on a cron. Sounds simple; the three common pitfalls are
**clock skew**, **stuck previous runs**, and **idempotency on retry**.

## Basic cron rule

```toml
[rule]
name = "nightly-backup-notify"
enabled = true

[trigger]
type = "Cron"
expression = "0 2 * * *"        # 02:00 every day, local time
timezone = "America/Los_Angeles"  # explicit beats local default

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "YOUR_CHAT_ID"
text = "Nightly backup window starts now."
```

The `expression` is a 5-field cron expression (minute, hour, day of
month, month, day of week). See [crontab.guru](https://crontab.guru)
to test patterns.

`timezone` is explicit; default is UTC. If you want "02:00 in my
timezone", set it. The scheduler uses chrono-tz, so any IANA name
works (`Europe/Berlin`, `Asia/Tokyo`, `UTC`).

## Skip if the previous run hasn't finished

The default scheduler doesn't track per-rule in-flight state. If
your 02:00 job takes 90 minutes and you scheduled it every hour,
you'll get parallel runs.

Use session memory as a poor-man's mutex:

```toml
[[conditions]]
type = "SessionFieldEquals"
session_key = "cron:nightly-backup"
field = "in_progress"
value = false

[[actions]]
type = "Transform"
op = "SetSessionField"

[actions.params]
session_key = "cron:nightly-backup"
field = "in_progress"
value = true

# ... your real actions ...

# At the end, release the lock:
[[actions]]
type = "Transform"
op = "SetSessionField"

[actions.params]
session_key = "cron:nightly-backup"
field = "in_progress"
value = false
```

This isn't a real mutex (two simultaneous reads could both pass the
condition before either writes the `true`). For real exclusion,
use the `Once` action type:

```toml
[[actions]]
type = "Once"
key = "nightly-backup"

[[actions.then]]
type = "RunConnector"
# ... the actual work ...
```

`Once` is atomic at the runtime layer. Two triggers firing at the
same instant only one wins.

## Skip if a recent run succeeded

For "send a daily reminder, but don't repeat if I already got one":

```toml
[[conditions]]
type = "SessionFieldOlderThan"
session_key = "cron:daily-reminder"
field = "last_success_at"
duration = "20h"
```

The trigger fires every 24h, but the condition checks "did the last
success happen >20h ago". Misfires due to scheduler delay (the
heartbeat is 1-second granularity) don't double-fire.

## Clock skew

`Cron` expressions are evaluated against the daemon's local clock
(modified by `timezone`). If the daemon's clock drifts:

- A few seconds: harmless. The scheduler tolerates skew up to its
  tick granularity (1 second).
- Minutes: you'll see triggers fire at the wrong wall-clock time,
  or miss firings around DST transitions.
- Hours: catastrophic, but you have bigger problems.

For high-confidence cron, run NTP. Or use the `Once + every 24h via
heartbeat tick` pattern below.

## Heartbeat-based pseudo-cron

If you don't trust cron parsing or DST handling, do it with the
heartbeat:

```toml
[trigger]
type = "Heartbeat"

[[conditions]]
type = "And"

[[conditions.children]]
type = "TimeInRange"
start = "02:00"
end = "02:01"
timezone = "America/Los_Angeles"

[[conditions.children]]
type = "SessionFieldOlderThan"
session_key = "cron:nightly-pseudo"
field = "last_success_at"
duration = "23h"
```

`Heartbeat` fires every `heartbeat_interval_secs` (default 1800 =
30 minutes). The conditions narrow to "during 02:00-02:01 local AND
hasn't run in 23h". Misses the precise 02:00 instant, but provides
a window so the heartbeat catches it.

Lower the heartbeat interval to ~60 if you want minute-precision.

## Capturing output

For "run something, capture the output, do something with it":

```toml
[[actions]]
type = "RunConnector"
connector = "connector-shell"
action = "exec"

[actions.params]
command = ["/usr/local/bin/backup.sh"]
timeout_secs = 3600
capture_stdout = true
capture_stderr = true

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "YOUR_CHAT_ID"
text = "Backup done.  Output:\n```\n${actions.0.stdout}\n```"
parse_mode = "Markdown"
```

`${actions.0.stdout}` interpolates the first action's stdout. Index
is into the actions array of the same rule.

## Heartbeat config

Disable the heartbeat entirely:

```toml
heartbeat_interval_secs = 0
```

Set it shorter:

```toml
heartbeat_interval_secs = 60
```

Heartbeat fires `InternalEvent::Heartbeat`. Rules with `trigger.type =
"Heartbeat"` listen for it.

## Gotchas

- **Cron without timezone defaults to UTC.** If you wrote `0 9 * * *`
  expecting "9am my time", check the timezone field.
- **DST transitions** can cause cron expressions to fire twice or
  zero times on the transition day. Use the heartbeat-based pattern
  if this matters.
- **`Once` is per-daemon**, not global. If you run two daemons with
  the same rule, both will fire. (Cross-daemon coordination is a
  Phase 3 / Veilid concern.)
- **`SessionFieldOlderThan` reads from SQLite.** It's fine for daily
  cadence; if you're doing per-second decisions this gets expensive.
- **Shell actions need the command on the allow-list** in
  `[shell] allowed_commands`. See [`docs/reference/connectors/shell.md`](../reference/connectors/shell.md).
