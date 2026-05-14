# Dedupe + Extract (Phase A)

Two new `Action` variants in the rule engine that together make
**polling recipes** practical: a recipe fires every N minutes, pulls
a feed / HTML page / JSON endpoint, extracts items, and only acts
on items it hasn't seen before.

`Action::Extract` parses bytes into structured data. `Action::Dedupe`
short-circuits the chain when an item's key has been seen before.
Use them in sequence: extract the items, then dedupe each one.

This is Phase A in the cooperation-alignment roadmap. Phase 0.4
gave us the scoping primitives (cooperation context with formation
ids); Phase A wires them into the polling story.

## When you need this

Polling-style recipes — anything that asks "is there new stuff?"
on a schedule:

| Recipe pattern | What it polls | What it dedupes on |
|---|---|---|
| RSS broadcast | RSS / Atom feed | Item GUID |
| Page-change watcher | A web page | Hash of normalised body |
| Calendar feed reminder | iCalendar `.ics` URL | Event UID |
| GitHub release pinger | `/repos/{r}/releases` JSON | Release tag |
| Job-board alert | LinkedIn / Indeed scrape | Job posting id |

Each runs hourly (or whatever Cron says), but you want the rule to
fire *only* on items the bot hasn't seen. Phase A's dedupe is what
makes that work.

Without dedupe: the bot reposts the same RSS items every hour.
With dedupe: it posts each item once.

## `Action::Extract` — bytes → structured data

```toml
[[actions]]
type = "Extract"
source = "${last_http_output.body}"
kind = "feed"
```

`source` is a template — usually `${last_http_output.body}` from a
preceding HTTP GET, or `${last_browser_output.html}` from a
`connector-browser` navigation. The runtime resolves the template,
then runs the extraction.

### `ExtractKind` variants

| Kind | Use for | Output |
|---|---|---|
| `readability` | HTML article body — strip nav/ads | `{ title, byline, content, text_content, excerpt }` |
| `css` | HTML with stable CSS structure | `{ field_name: extracted_value, ... }` per author's schema |
| `json_path` | JSON API response | `{ field_name: value }` per JSONPath schema |
| `feed` | RSS / Atom / JSON Feed | `{ entries: [{ id, title, link, content, published }, ...] }` |
| `ical` | iCalendar `.ics` URL | `{ events: [{ uid, summary, start, end, location }, ...] }` filtered by `window_days` |
| `llm_schema` | AI-driven structured extraction | per JSON Schema author declares |
| `passthrough` | Source already typed | the source unchanged |

Source: `crates/springtale-core/src/rule/action.rs::ExtractKind`.

### CSS extraction specifics

The `css` kind takes a `schema` map of `{ field_name: selector }`:

```toml
[actions.kind]
type = "css"

[actions.kind.schema]
title    = "h1.article-title"
author   = "span.byline"
links    = "a.tag :all"          # all matches, as array
hero_img = "img.hero @src"        # the @src attribute, not text
```

Suffix conventions:

- `selector :all` — returns an array of all matches' text content.
- `selector @attr` — returns the named attribute instead of text.

Implementation: `scraper` crate (html5ever + Servo selectors).

### JSONPath specifics

```toml
[actions.kind]
type = "json_path"

[actions.kind.schema]
releases = "$.[*].tag_name"
authors  = "$.[*].author.login"
urls     = "$.[*].html_url"
```

Implementation: RFC 9535 JSONPath. Each path resolves to either a
scalar (one match) or an array (multiple matches).

### LLM-schema specifics

```toml
[actions.kind]
type = "llm_schema"

[actions.kind.schema]
# The JSON Schema you want the AI to produce.
type = "object"
properties.headline.type = "string"
properties.tags.type = "array"
properties.tags.items.type = "string"
required = ["headline", "tags"]
```

The configured AI adapter must support structured outputs (preflight
checks for adapter capability before deploy — see
[`recipe-authoring-tools.md`](recipe-authoring-tools.md)).

## `Action::Dedupe` — short-circuit the chain

```toml
[[actions]]
type = "Dedupe"
key = "${last_extract_output.entries.0.id}"
bucket = "rss-items"
history = 10000        # optional, default 10_000
```

Semantics:

1. Runtime resolves `key` against the current chain context.
2. Hashes the plaintext key with blake3, gets a hex digest.
3. Looks up `(formation_id, rule_id, bucket, key_hash)` in the
   `dedupe_seen` table.
4. **Hit** → chain short-circuits with `ChainError::Suppressed`;
   execution status `empty`.
5. **Miss** → insert the row, chain continues to the next step.
6. LRU prune keeps the bucket bounded at `history` entries (default
   10,000); oldest entries dropped on insert.

The chain status `empty` is important — it's a successful run that
produced no work. The executions log and drift detector distinguish
empty from error (see
[`executions-and-drift.md`](executions-and-drift.md)).

### Privacy posture

The plaintext key never touches disk. Only the blake3 hex digest is
stored. Why this matters:

- An RSS feed item id can be a URL containing the user's mailbox
  (`mailto://...`), the sender's domain, or a unique-per-recipient
  tracking id. Storing the plaintext exposes the social graph.
- A calendar event UID can encode the meeting topic or attendees.
- Hashing keeps the dedupe state PII-free per the project's data-
  minimisation rule. This matches the executions log (sizes only)
  and the workspace directory (names only, never rosters).

## Scoping (Phase 0.4 cooperation alignment)

The `dedupe_seen` table is keyed by `(formation_id, rule_id, bucket,
key_hash)`. Two scopes:

- **`formation_id NULL`** — the rule is global. One dedupe state
  across the whole daemon.
- **`formation_id NOT NULL`** — the rule is scoped to a specific
  formation instance. Two formations running the same rule see
  **independent** dedupe state.

This is critical for the cooperation invariant: a single rule
deployed across two formations shouldn't have one formation's
dedupe seen-set silently suppress the other formation's first-time
items.

The dispatcher passes `ExecutionContext::formation_id` to the
dedupe step automatically. Recipe authors don't pick the scope; it
follows the cooperation envelope.

## The common chain shape

```toml
# Recipe: poll an RSS feed, post new items to Telegram.

[trigger]
type = "Cron"
expression = "*/15 * * * *"      # every 15 minutes

[[actions]]
type = "RunConnector"
connector = "connector-http"
action = "get"
[actions.params]
url = "${rss_url}"

[[actions]]
type = "Extract"
source = "${last_run_connector_output.body}"
kind = "feed"

# Now we have last_extract_output.entries[].

[[actions]]
type = "ForEach"
input = "${last_extract_output.entries}"

[[actions.then]]
type = "Dedupe"
key = "${item.id}"
bucket = "rss-feed"

[[actions.then]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"
[actions.then.params]
chat_id = "${target_chat_id}"
text = "${item.title}\n${item.link}"
```

What this does on each fire:

1. HTTP-GET the RSS URL.
2. Extract entries.
3. For each entry: dedupe against `bucket = "rss-feed"`.
4. If new → send to Telegram. If seen → short-circuit, status `empty`.

After the first run, only newly-published items hit Telegram.

### `last_*_output` aliases

The chain's step outputs are addressable as `last_<action_kind>_output`:

- `last_run_connector_output` — most recent connector action result
- `last_extract_output` — most recent extraction result
- `last_transform_output` — most recent transform
- `last_ai_complete_output` — most recent AI completion

These are convenience aliases for the most recent step of that kind.
The full chain state is also available as `chain.steps[N]`.

## Authoring patterns

### Buckets group related dedupe state

```toml
# Two rules sharing the same dedupe set:
[[actions]]
type = "Dedupe"
key = "${item.id}"
bucket = "github-releases"
```

Two different rules both writing into the `"github-releases"`
bucket (same formation) share the seen-set. Useful for "post
releases to Bluesky AND Discord" recipes where you want each
release posted to both, but only once.

### History sizing

Default `history = 10000`. Tune by:

- **Volume of incoming items.** A page-change watcher with one
  item per fire can use `history = 100`. An RSS feed pulling 50
  items per refresh probably wants `1000+`.
- **Time horizon.** If items can be replayed after N days
  (common for podcasts re-publishing old episodes), set history >
  the expected churn rate over N days.

### Don't dedupe trigger payloads

Dedupe is for *items extracted from* the trigger, not the trigger
itself. The webhook / cron infrastructure handles trigger-level
idempotency separately (each fire has a unique invocation id).
Using `Dedupe` on `${trigger.id}` defeats the trigger's own
once-per-fire guarantee.

## What you see in the dashboard

The executions log records each fire's terminal status. A
dedupe-aware polling recipe produces a histogram like:

```
   success  ▓▓
   empty    ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
   error    ▓
```

Most runs are `empty` — the feed didn't have anything new. The
occasional `success` is a fresh item. `error` rows mean the HTTP
fetch failed, the extraction threw, or the downstream action
errored — `error_kind` distinguishes which.

A degrading drift report on a polling recipe usually means the
*data source* is breaking (RSS feed URL moved, CSS selectors no
longer match), not the recipe itself.

## Gotchas

- **`Action::Dedupe` short-circuits the chain.** Steps after the
  dedupe step do not run when the key is seen. Put expensive work
  *after* the dedupe to save fuel.
- **`Action::Extract` failure is `ChainError`, not `Suppressed`.**
  A malformed feed errors out the chain with
  `status = "error"`, `error_kind = "parse_error"`. The dedupe state
  isn't touched — the failed item gets a fresh chance next fire.
- **`history` is per-bucket, not per-rule.** Two rules writing
  into the same bucket share the LRU cap. Pick bucket names that
  match the data lifetime.
- **Dedupe is local-only.** A second daemon polling the same feed
  doesn't see this daemon's dedupe state. Cross-daemon dedup is a
  Phase 3 (Veilid) concern.
- **`bucket` is plain text in the recipe TOML.** Pick a stable name
  — renaming the bucket throws away the seen-set.
- **`source` resolution is strict.** A typo in
  `${last_run_connector_output.body}` errors at chain-evaluation
  time, not parse time. The preflight check
  (W1.D — see [recipe-authoring-tools.md](recipe-authoring-tools.md))
  catches the common typos before deploy.

## See also

- [`docs/guide/recipes.md`](recipes.md) — the recipes layer that
  uses these actions
- [`docs/guide/executions-and-drift.md`](executions-and-drift.md) —
  observability for what your dedupe is doing
- [`docs/reference/recipes-format.md`](../reference/recipes-format.md) —
  recipe schema reference
- `crates/springtale-core/src/rule/action.rs` — `Action::Extract` /
  `Action::Dedupe` source
- `crates/springtale-store/src/schema/sql/dedupe.sql` — schema
