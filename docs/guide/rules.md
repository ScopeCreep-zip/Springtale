# Rules

Rules are Springtale's core automation unit. A rule says: "when THIS happens, if THESE conditions are true, do THAT." Rules are authored in TOML, stored in SQLite, and evaluated by the rule engine with zero AI required.

## 1. Anatomy of a Rule

```
  ┌──────────────────────────────────────────────────────────────┐
  │                           Rule                               │
  │                                                              │
  │  id:      UUID (auto-generated)                              │
  │  name:    "stream-announce"                                  │
  │  status:  enabled | disabled | draft                         │
  │  version: monotonic u64                                      │
  │                                                              │
  │  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐  │
  │  │   Trigger    │  │  Conditions  │  │     Actions       │  │
  │  │  (exactly 1) │  │  (0 or more) │  │  (1 or more)      │  │
  │  │              │  │  all must     │  │  run in sequence   │  │
  │  │  "when this  │  │  pass (AND)  │  │  or chain          │  │
  │  │   happens"   │  │              │  │                    │  │
  │  └──────────────┘  └──────────────┘  └───────────────────┘  │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
```

*Fig. 1. Rule structure. One trigger, zero or more conditions, one or more actions.*

---

## 2. Triggers

A trigger defines what event starts the rule. One trigger per rule.

**TABLE I. TRIGGER TYPES**

| Type | What fires it | Key fields |
|------|--------------|------------|
| `Cron` | Timer on a cron schedule | `expression`: cron string (e.g., `"0 */6 * * *"`) |
| `FileWatch` | Filesystem change detected | `path`: directory to watch, `event`: `"create"`, `"modify"`, `"delete"`, or `"any"` |
| `Webhook` | HTTP POST to webhook endpoint | `path`: URL path (e.g., `"/deploy"`) |
| `ConnectorEvent` | A connector emits an event | `connector`: connector name, `event`: trigger name |
| `SystemEvent` | Internal system event | `event`: event name |

Example triggers in TOML:

```toml
# Fire when a Kick stream goes live
[trigger]
type = "ConnectorEvent"
connector = "connector-kick"
event = "stream_live"

# Fire every 6 hours
[trigger]
type = "Cron"
expression = "0 */6 * * *"

# Fire when a file is created in /data/inbox
[trigger]
type = "FileWatch"
path = "/data/inbox"
event = "create"
```

---

## 3. Conditions

Conditions filter when a rule fires. All conditions must pass (AND logic at the top level). If a rule has no conditions, it fires on every trigger match.

**TABLE II. CONDITION TYPES**

| Type | What it checks | Example |
|------|---------------|---------|
| `FieldEquals` | Exact value match on a payload field | `field = "trigger.event"`, `value = "stream_live"` |
| `Contains` | Substring match | `field = "trigger.title"`, `value = "minecraft"` |
| `Regex` | Regex pattern match | `field = "trigger.text"`, `pattern = "^!\\w+"` |
| `TimeInRange` | Current time within range | `start = "09:00"`, `end = "17:00"` |
| `DayOfWeek` | Current day matches | `days = [1, 2, 3, 4, 5]` (Mon-Fri) |
| `And` | All sub-conditions pass | `conditions = [...]` |
| `Or` | Any sub-condition passes | `conditions = [...]` |
| `Not` | Sub-condition fails | `condition = { ... }` |

Conditions support dotted field paths into the trigger payload. For example, `trigger.sender.name` resolves through nested JSON objects. Array indexing works too: `trigger.commits.0.message`.

Constraints: max nesting depth of 8 levels. Regex patterns limited to 1MB (prevents ReDoS).

```toml
# Only fire during business hours on weekdays
[[conditions]]
type = "TimeInRange"
start = "09:00"
end = "17:00"

[[conditions]]
type = "DayOfWeek"
days = [1, 2, 3, 4, 5]
```

---

## 4. Actions

Actions are what the rule does when it fires. They run in sequence.

**TABLE III. ACTION TYPES**

| Type | What it does | Key fields |
|------|-------------|------------|
| `RunConnector` | Execute a connector action | `connector`, `action`, `params` (key-value map) |
| `SendMessage` | Emit a text message | `text` |
| `WriteFile` | Write content to a file | `destination`, `content`, `delete_source` (bool) |
| `RunShell` | Execute a shell command | `command` |
| `Notify` | Send a notification | `title`, `body` |
| `Chain` | Run nested actions in sequence | `steps` (list of actions, max depth: 4) |
| `Transform` | Apply a data transformation | `operation`, `params` |
| `Delay` | Wait before next action | `seconds` |
| `AiComplete` | Optional AI call (Phase 2) | `prompt`, `adapter` (optional) |

### 4.1. Template Variables

Action fields support `${trigger.field}` template syntax. Variables resolve against the trigger event payload at dispatch time.

```toml
[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.params]
text = "${trigger.username} is live on Kick: ${trigger.title}"
```

Available variables depend on the trigger. For example, a `connector-kick` `stream_live` trigger provides `${trigger.broadcaster.username}`, `${trigger.title}`, `${trigger.started_at}`.

---

## 5. The Pipeline

Under the hood, actions flow through a pipeline of stages. Each stage reads from and writes to a `PipelineContext` — a data bag carrying input, output, errors, and metadata.

```
  TriggerEvent
       │
       v
  ┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
  │ Stage 1  │────>│ Stage 2  │────>│ Stage 3  │────>│ Stage N  │
  │          │     │          │     │          │     │          │
  │ reads    │     │ reads    │     │ reads    │     │ reads    │
  │ ctx.input│     │ ctx.output│    │ ctx.output│    │ ctx.output│
  │ writes   │     │ writes   │     │ writes   │     │ writes   │
  │ ctx.output│    │ ctx.output│    │ ctx.output│    │ ctx.output│
  └──────────┘     └──────────┘     └──────────┘     └──────────┘
       │                                                   │
       │           PipelineContext flows through            │
       └───────────────────────────────────────────────────┘
```

*Fig. 2. Pipeline stage composition. Stages execute left-to-right. First failure short-circuits the pipeline.*

The `PipelineContext` carries:
- `trace_id` — UUID for tracing a request through the system
- `input` — original trigger payload
- `output` — current data (modified by each stage)
- `errors` — collected error messages
- `retry_count` — how many times this pipeline has retried
- `chain_depth` — current nesting depth (max 4)

---

## 6. Worked Examples

### 6.1. Cross-post: Kick Stream → Bluesky Post

When a Kick stream goes live, announce it on Bluesky. No AI needed.

```toml
[rule]
name = "stream-announce"

[trigger]
type = "ConnectorEvent"
connector = "connector-kick"
event = "stream_live"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.params]
text = "${trigger.broadcaster.username} is live: ${trigger.title}"
```

### 6.2. File Organizer: New File → Move to Archive

When a file appears in `/data/inbox`, move it to `/data/archive`.

```toml
[rule]
name = "auto-archive"

[trigger]
type = "FileWatch"
path = "/data/inbox"
event = "create"

[[actions]]
type = "WriteFile"
destination = "/data/archive/${trigger.filename}"
content = ""
delete_source = true
```

### 6.3. Scheduled Backup: Cron → Shell Command

Run a backup script every day at 3 AM.

```toml
[rule]
name = "daily-backup"

[trigger]
type = "Cron"
expression = "0 3 * * *"

[[actions]]
type = "RunShell"
command = "backup-script"
```

### 6.4. GitHub Webhook → Multiple Actions

When a PR is opened, create a Bluesky post AND post a GitHub comment.

```toml
[rule]
name = "pr-announce"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "pull_request_opened"

[[conditions]]
type = "FieldEquals"
field = "trigger.repository"
value = "ScopeCreep-zip/Springtale"

[[actions]]
type = "Chain"

[[actions.steps]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.steps.params]
text = "New PR on Springtale: ${trigger.title} by ${trigger.author}"

[[actions.steps]]
type = "RunConnector"
connector = "connector-github"
action = "post_comment"

[actions.steps.params]
owner = "ScopeCreep-zip"
repo = "Springtale"
issue_number = "${trigger.number}"
body = "Thanks for the PR! Reviewing shortly."
```

---

## References

- [1] CLI rule commands: [reference/cli.md](../reference/cli.md)
- [2] API rule endpoints: [reference/api.md](../reference/api.md)
- [3] Connector triggers and actions: [reference/connectors/](../reference/connectors/)
- [4] Full rule engine spec: `docs/current-arch/ARCHITECTURE.md` §6.1
