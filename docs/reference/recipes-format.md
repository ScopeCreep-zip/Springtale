# Recipe format

The wire-format and data-shape reference for recipes. For the
conceptual model and when to use recipes, read
[`guide/recipes.md`](../guide/recipes.md). For authoring walkthroughs,
[`cookbook/authoring-a-recipe.md`](../cookbook/authoring-a-recipe.md).

This document describes the canonical JSON shape that the HTTP API
and Tauri IPC emit + accept. The Rust source of truth is
`crates/springtale-runtime/src/operations/recipes/types.rs`.

## `Recipe`

The top-level catalogued recipe. JSON shape:

```json
{
  "id": "telegram-echo",
  "name": "Telegram echo bot",
  "description": "Repeats every message back.",
  "icon_id": "telegram-sprite",
  "category": "messaging",
  "tags": ["telegram", "no-ai"],
  "connectors_used": ["connector-telegram"],
  "ai_required": false,
  "difficulty": "quick",
  "source": { "kind": "builtin" },
  "inputs": [ ... InputField ... ],
  "blueprint": { ... RecipeBlueprint ... }
}
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `id` | string | ✓ | Kebab-case. Built-ins use literal slugs (`"telegram-echo"`); user recipes use UUIDs. |
| `name` | string | ✓ | Plain text rendered as the card title. |
| `description` | string | ✓ | Plain text rendered as the card subtitle. |
| `icon_id` | string | ✓ | Sprite id from `@springtale/ui`'s sprite map. |
| `category` | `RecipeCategory` | ✓ | See enum below. |
| `tags` | string[] | ✓ | Free-form filter tags. |
| `connectors_used` | string[] | ✓ | The connector names the blueprint will touch. Used for the "connectors required" preflight check. |
| `ai_required` | bool | ✓ | `true` if the recipe needs a working AI adapter to operate meaningfully. The card surfaces this so a NoopAdapter user can self-filter. |
| `difficulty` | `Difficulty` | ✓ | See enum below. |
| `source` | `RecipeSource` | ✓ | Tagged union; see below. |
| `inputs` | `InputField[]` | ✓ | Author-declared input fields. May be empty for a no-input recipe. |
| `blueprint` | `RecipeBlueprint` | ✓ | What `apply_recipe` runs. |

## `RecipeCategory`

```
"messaging" | "coding" | "web" | "ai_assistant"
    | "daily" | "safety_privacy" | "custom"
```

UI labels (from `RecipeCategory::label()`):

| Variant | Label |
|---|---|
| `messaging` | Messaging |
| `coding` | Coding |
| `web` | Web |
| `ai_assistant` | AI assistants |
| `daily` | Daily tasks |
| `safety_privacy` | Safety & Privacy |
| `custom` | Custom |

## `Difficulty`

```
"quick" | "standard" | "power"
```

- **`quick`** — 3-click deploy.
- **`standard`** — 5-ish clicks, a few optional knobs.
- **`power`** — multi-agent, intricate trigger graph.

## `RecipeSource`

Tagged union; the `kind` field discriminates:

```json
{ "kind": "builtin" }

{ "kind": "user" }

{
  "kind": "community",
  "author": "alice@example.com",
  "signature": "base64-ed25519-sig"
}
```

| Variant | Trust |
|---|---|
| `builtin` | Compiled into the daemon binary. Highest trust. |
| `user` | Authored locally by the user. Trust = the user's trust in themselves. |
| `community` | Future marketplace. Carries author + Ed25519 signature; sentinel verifies before install. Wire-shape only today. |

## `InputField`

```json
{
  "id": "bot_token",
  "label": "Telegram bot token",
  "kind": { "kind": "secret" },
  "visibility": "required",
  "default": null,
  "hint": "Get this from @BotFather on Telegram."
}
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `id` | string | ✓ | Used to address the field in `${input_id}` substitution. |
| `label` | string | ✓ | Plain-text label rendered next to the input. |
| `kind` | `FieldKind` | ✓ | Tagged union; see below. |
| `visibility` | `FieldVisibility` | ✓ | See below. |
| `default` | JSON value or null | — | Initial value. `null` means empty. |
| `hint` | string or null | — | Hint rendered under the input. Plaintext only — never user-supplied HTML. |

## `FieldKind`

Tagged union. The `kind` field is the discriminator.

```json
{ "kind": "text" }
{ "kind": "secret" }
{ "kind": "number" }
{ "kind": "bool" }
{ "kind": "url" }
{ "kind": "select", "options": [{ "value": "a", "label": "A" }, ...] }
{ "kind": "cron" }
{ "kind": "css_selector", "sample_url": "watch_url" }
{ "kind": "json_schema", "example": { ... } }
{ "kind": "workspace_target", "connector": "connector-telegram", "kinds": ["channel"] }
```

| Variant | Rendering | Storage |
|---|---|---|
| `text` | Plain text input | Plain value |
| `secret` | Password input (masked) | Vault (`Secret<String>`) |
| `number` | Numeric input | Plain value |
| `bool` | Toggle | Plain value |
| `url` | Text input, validated against http/https scheme on submit | Plain value |
| `select` | Dropdown of the `options` array | Plain value (the chosen `value`) |
| `cron` | Cron input (5/6-field) with a `CronFrequencyChip`: 🔴 sub-minute (preflight blocking), 🟡 1–4 min (warning), 🟢 ≥5 min (verified), plus a next-5-fire-times preview. Classification comes from the backend's `check_schedule_frequency` — the frontend never invents thresholds. | Cron string |
| `css_selector` | CSS selector input. `sample_url` names the recipe input whose URL the Tauri selector picker loads (Phase B activates the picker; today it renders as text with a "test against URL" probe). Optional — omitted means the picker prompts inline. | Selector string |
| `json_schema` | JSON Schema for the AI extraction step. Monaco-style editor planned (Phase B); textarea today. Optional `example` is a sample payload shown alongside. | JSON Schema object |
| `workspace_target` | Destination dropdown over the formation's discovered workspaces (`mental_model_workspaces`), filtered by `connector` and optional `kinds` (e.g. `["channel"]` excludes DMs). Includes 🔍 Scan (active discovery via `discover_destinations`), 🎯 Onboard (Telegram deep-link), and ✏️ Manual entry. Resolves at deploy time to the raw destination id, so `${chat_id}` / `${to}` substitution is unchanged. | Destination id string |

`SelectOption` shape:

```json
{ "value": "internal-id", "label": "User-friendly label" }
```

## `FieldVisibility`

```
"required" | "optional" | "advanced" | "baked"
```

| Variant | Where it shows |
|---|---|
| `required` | Always visible. Preflight blocks Deploy until filled. |
| `optional` | Behind "Show more options" chevron. |
| `advanced` | Behind "Show advanced" chevron. |
| `baked` | Never shown. Ships the default unchanged. |

## `RecipeBlueprint`

What `apply_recipe` runs against the running runtime.

```json
{
  "connector_configs": [ ConnectorConfigStep, ... ],
  "rules":             [ RuleStep, ... ],
  "ai_config":         AiConfigStep | null,
  "summary":           "Plain-language preview text" | null,
  "derived_inputs":    [ DerivedInputResolver, ... ]
}
```

All fields are `#[serde(default)]` and may be omitted; an empty
blueprint is valid (a no-op recipe).

`derived_inputs` lists derived-input resolvers (e.g. geocoding) that run
at deploy time, before placeholder substitution. Each resolver's
`target_input_id` is treated as a declared placeholder, so the rule TOML
can reference it — this is how a universal recipe accepts a free-text
city and substitutes concrete latitude/longitude.

### `ConnectorConfigStep`

```json
{
  "connector_name": "connector-telegram",
  "config": {
    "bot_token": "${bot_token}",
    "polling": { "enabled": true }
  }
}
```

`config` is a JSON value template. Every string leaf is passed
through `${input_id}` substitution before
`upsert_connector_config()` runs. Non-string leaves pass through
unchanged.

### `RuleStep`

```toml
# RuleStep.toml — substituted, then parsed.
[trigger]
type = "Connector"
connector = "connector-telegram"
event = "message_received"

[[actions]]
type = "Connector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${trigger.chat.id}"
text = "${welcome_text}"
```

The TOML is substituted before parse. `${input_id}` placeholders that
correspond to recipe inputs are filled with the user's values;
`${trigger.foo}` patterns are passed through (they're rule-engine
variables that resolve at trigger time).

### `AiConfigStep`

```json
{
  "target": "ai:formation:research-squad",
  "config": {
    "provider": "anthropic",
    "model": "claude-sonnet-4-6",
    "api_key": "${anthropic_api_key}"
  }
}
```

`target` is one of:

- `"ai:global"` — daemon-wide default
- `"ai:{agent_id}"` — single agent
- `"ai:formation:{id}"` — formation-scoped

## `${input_id}` substitution

Placeholders use `${input_id}` syntax. Resolved by
`apply_recipe::substitute_placeholders()` before any side effect.

Rules:

- Identifier must match a declared input on the recipe.
- Unknown placeholders → `ApplyError::UnknownPlaceholder` before any
  write happens.
- Required-but-empty input → `ApplyError::MissingRequiredInput`.
- JSON: every string leaf substituted; non-strings pass through.
- TOML: substituted as plain template before parse.

Not yet supported:

- **Computed / derived placeholders** like `${url_host}` from
  `${url}`. Tracked in `apply.rs` as future work.
- **Conditional blueprint steps** (e.g. "only upsert this connector
  if the user set Optional input X"). Authors work around by making
  the relevant field Required.

## `RecipeFilter` (library query)

```json
{
  "query": "telegram",
  "category": "messaging",
  "tags": ["no-ai"],
  "sources": ["builtin", "user"],
  "favorites_only": false,
  "limit": 50,
  "sort": "recommended"
}
```

All fields optional. Filtering and sorting are server-side. Server
caps `limit` at 100.

### `RecipeSourceFilter`

```
"builtin" | "user" | "community"
```

Combine in the `sources` array (AND across other filters; OR within
sources).

### `RecipeSort`

```
"recommended" | "name" | "recent"
```

- `recommended` (default): built-ins first, sorted by Difficulty
  (`quick` → `power`).
- `name`: alphabetical by name.
- `recent`: most recently used first (user-recent, then everything else).

## `ApplyReport`

Returned by `POST /recipes/{id}/apply`. Indicates what landed:

```json
{
  "recipe_id": "telegram-echo",
  "connectors_configured": ["connector-telegram"],
  "rules_created": ["welcome-echo-rule"],
  "ai_configured": true,
  "summary": "Plain-language post-deploy summary"
}
```

The list shapes correspond to which blueprint steps succeeded.
Partial-failure: the operation is **not** transactional across all
steps. Future work adds compensating-write rollback.

## TOML export / import

Recipes round-trip through TOML for portability:

- `GET /recipes/{id}/export` → TOML representation
- `POST /recipes/import` → parse TOML back into a `Recipe`, store as
  `User`

The TOML mirrors the JSON shape with snake_case keys. The
canonicalisation rule is the serde default for `Type`-derived
structs — see `crates/springtale-runtime/src/operations/recipes/types.rs`
for exact serialisation.

> User recipe storage (`recipes_user` table) is not yet shipped —
> `POST /recipes/import` returns `OperationError::NotSupported`
> until W2.B lands. Tracked in `docs/arch/AUDIT-NOTES.md`.

## ApplyError variants

```rust
enum ApplyError {
    RecipeNotFound(String),
    MissingRequiredInput(String),
    UnknownPlaceholder(String),
    InvalidRuleToml(String),
    Operation(OperationError),
}
```

The first three are pre-flight failures with no side effects.
`InvalidRuleToml` and `Operation` may have already executed earlier
blueprint steps — check the returned `ApplyReport` for partial state.

## See also

- [`guide/recipes.md`](../guide/recipes.md) — concept + flow
- [`reference/api.md §3.15`](api.md) — the 16 HTTP endpoints
- [`cookbook/authoring-a-recipe.md`](../cookbook/authoring-a-recipe.md) — worked example
- [`tutorials/04-deploy-recipe.md`](../tutorials/04-deploy-recipe.md) — first-time deploy walkthrough
- `crates/springtale-runtime/src/operations/recipes/` — source
