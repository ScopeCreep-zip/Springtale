# Recipe: Event fan-out

One trigger → many actions. Three sub-patterns:

1. **Static fan-out** — paste N actions, hardcode targets. Simplest.
2. **Iterate over a list** — `Chain` action with a list parameter.
3. **Dynamic fan-out** — query the list at trigger time.

## Static fan-out

Already shown in [tutorial 02](../tutorials/02-cross-platform-alerts.md).
N `[[actions]]` blocks, each independent. They dispatch in parallel
unless you mark one as `chain_depends_on`.

```toml
[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "push"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"
# ... params ...

[[actions]]
type = "RunConnector"
connector = "connector-slack"
action = "send_message"
# ... params ...

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"
# ... params ...
```

Each action gets its own dispatch row. If Telegram fails, Slack +
Bluesky still go through. Audit trail captures each.

## Iterate over a list

When you have "send DM to these 50 users":

```toml
[[actions]]
type = "Chain"
foreach = ["user1", "user2", "user3"]

[[actions.then]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.then.params]
chat_id = "${item}"
text = "Stream is live at https://kick.com/${trigger.broadcaster.username}"
```

`${item}` is the loop variable. `Chain` iterates serially with a
1-second backoff between iterations by default (configurable via
`chain_delay_ms`). For 50 users that's ~50 seconds total, well under
Telegram's per-bot rate limit (30 msg/sec to different chats).

## Read the list from a file

Subscriber lists usually live in a file, not in the rule:

```toml
[[actions]]
type = "Chain"
foreach_file = "/var/lib/springtale/subscribers.json"
foreach_jq = ".telegram_subscribers[]"

[[actions.then]]
type = "RunConnector"
# ... uses ${item} ...
```

`foreach_file` reads the JSON. `foreach_jq` is a jq expression that
extracts the iterable. Editing the file is the subscriber-list
management; no daemon restart needed.

The path must be on `[filesystem] allowed_read_paths` for
`connector-filesystem` to read it.

## Read the list from a connector

Subscribers in GitHub Issues, or Discord roles, or a Notion page:

```toml
# Step 1: query the list.
[[actions]]
type = "RunConnector"
connector = "connector-github"
action = "list_subscribers_to_label"

[actions.params]
owner = "myorg"
repo = "myrepo"
label = "subscribed"

# Step 2: fan out using the output.
[[actions]]
type = "Chain"
foreach_from_action = 0       # index into [[actions]] above
foreach_jq = ".[].handle"

[[actions.then]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.then.params]
chat_id = "${item}"
text = "${trigger.title}"
```

`foreach_from_action` references the output of an earlier action in
the same rule. The schema is "whatever that action returns" — see
the per-connector reference docs for return shapes.

## Sequential vs parallel

By default, actions in the same rule dispatch in parallel. Within a
`Chain.foreach`, items run sequentially with a delay between.

To force sequential across separate actions:

```toml
[[actions]]
type = "RunConnector"
# ... first action ...

[[actions]]
type = "RunConnector"
chain_depends_on = 0
# ... runs after action 0 completes successfully ...
```

`chain_depends_on` indexes into the actions array. The dependent
action only fires if the dependency succeeded.

## Fan-out with per-recipient personalization

```toml
[[actions]]
type = "Chain"
foreach_file = "/var/lib/springtale/subscribers.json"
foreach_jq = ".subscribers[]"

[[actions.then]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.then.params]
chat_id = "${item.chat_id}"
text = "Hi ${item.name} — ${trigger.title}"
```

`${item.x}` works when each iterated item is an object, not a
string.

## Concurrency control

For high-volume fan-outs, the default 1-second backoff is too
generous. Set explicitly:

```toml
[[actions]]
type = "Chain"
foreach = [...]
chain_delay_ms = 50              # 20 messages/sec
chain_max_concurrent = 10        # up to 10 in-flight at once
chain_failure_policy = "continue"  # don't stop on first failure
```

`chain_failure_policy = "abort"` stops the chain on first failure.
Default is `continue` for fan-out shape.

## Audit trail

Every dispatched action lands in the `audit_trail` table with:

- `rule_id`
- `action_index` (which `[[actions]]` block in the rule)
- `chain_iteration` (which iteration within a Chain)
- `verdict` (Go/Throttle/Pause/Quarantine)
- `outcome` (success/error/timeout)

Query example:

```sql
SELECT chain_iteration, outcome
FROM audit_trail
WHERE rule_id = (SELECT id FROM rules WHERE name = 'stream-fanout')
  AND created_at > datetime('now', '-1 hour')
ORDER BY chain_iteration;
```

Useful for "which subscribers' deliveries failed last batch".

## Gotchas

- **Per-platform rate limits.** Telegram is 30 msg/sec to different
  chats but 1 msg/sec to the same chat. Discord is 50/sec global.
  Slack is workspace-rate-limited. Tune `chain_delay_ms` to your
  most-restrictive target.
- **`Chain` doesn't track success across daemon restarts.** If the
  daemon dies mid-fan-out, the remaining iterations are lost.
  Mitigation: chunk large fan-outs into batches with `Once` keys per
  batch so a retry after restart resumes from the right place.
- **Sentinel rate limiter applies per-connector.** A large fan-out
  to one connector may trip its rate limit and pause the rule. See
  `[sentinel] rate_limits` per-connector in [`docs/reference/configuration.md`](../reference/configuration.md).
- **Audit trail growth.** A 1000-subscriber fan-out writes 1000
  audit rows. Set `[sentinel] audit_retention_days` accordingly.
