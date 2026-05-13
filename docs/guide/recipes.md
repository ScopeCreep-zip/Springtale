# Recipes

A recipe is a click-and-play blueprint that materialises a working
bot. Pick one from the library, fill in the required inputs, hit
Deploy, get a running automation. No TOML editing, no connector
setup wizards, no LLM-adapter plumbing.

If templates ([`templates.md`](templates.md)) are "start from a
working scaffold and edit it", recipes are "fill in 3 fields and run".

## When to use a recipe vs. a template

| Recipe | Template |
|---|---|
| Browseable library, filterable by category / tags / difficulty | One-shot scaffold per `springtale new <name>` |
| Backend decides which fields are required vs optional vs advanced | All fields exposed in the generated TOML |
| Deployed via UI (or `POST /recipes/{id}/apply`); no shell needed | Generated on disk; you edit and run |
| Best for first-time deploys + non-technical users | Best for power users + heavy customisation |
| Survives daemon restarts as a tracked deploy | Decoupled from the daemon after scaffold |

A recipe is a wrapper around the same primitives (connectors,
rules, AI config) — internally it composes the existing
`upsert_connector_config`, `create_rule`, and AI-config ops. Use
either; they coexist.

## Anatomy of a recipe

```
┌──────────────────────────────────────────────────────────┐
│  Recipe                                                  │
│                                                          │
│   id                  "telegram-echo"                    │
│   name                "Telegram echo bot"                │
│   description         "Repeats every message back."      │
│   icon_id             "telegram-sprite"                  │
│   category            Messaging                          │
│   tags                ["telegram", "no-ai"]              │
│   connectors_used     ["connector-telegram"]             │
│   ai_required         false                              │
│   difficulty          Quick                              │
│   source              Builtin                            │
│   inputs              [InputField, …]                    │
│   blueprint           RecipeBlueprint                    │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

Source of truth: `crates/springtale-runtime/src/operations/recipes/types.rs`.

### Categories

Seven categories ship: **Messaging**, **Coding**, **Web**, **AI
assistants**, **Daily tasks**, **Safety & Privacy**, **Custom**. The
library UI groups by category by default; users can filter via
`RecipeFilter { category: Some(…) }`.

### Difficulty

Three levels, surfaced as a chip in the card:

- **Quick** — 3-click deploy.
- **Standard** — 5-ish clicks, a few optional knobs.
- **Power** — multi-agent, intricate trigger graph, the works.

The library sorts Quick → Power by default (`RecipeSort::Recommended`).

### Sources

Where the recipe came from, used for the UI's trust badge:

- **Builtin** — compiled into the daemon binary. Highest trust;
  reviewed alongside the rest of the codebase.
- **User** — saved by the local user via the authoring flow (W2.B).
  Trust is the user's own trust in themselves.
- **Community** — wire-shaped for the future marketplace.
  Carries `author` + `signature`; the sentinel will verify the
  Ed25519 signature before install (W3.A). Not yet active in the
  current release.

## Input fields and progressive disclosure (W1.C)

Every recipe declares its inputs. Each `InputField` carries a
**visibility tier** the author sets explicitly:

| Tier | Where it surfaces |
|---|---|
| **Required** | Always visible. Preflight blocks the Deploy button until every Required field has a value (W1.D). |
| **Optional** | Hidden behind a "Show more options" chevron. Author has provided a sensible default. |
| **Advanced** | Hidden behind a "Show advanced" chevron. Power-user surface — most users won't see it. |
| **Baked** | Never shown. The recipe ships the default unchanged. Use for placeholders the author wants computed, or for fields explicitly documented in the recipe summary. |

The frontend renders what the backend tells it to — there's no
auto-classification heuristic, no "guess if this looks like a secret".
The author declares each tier per field. This matches the IFTTT
applet / Zapier Zap template / GitHub Actions input metadata model.

### Field types

```rust
enum FieldKind {
    Text,                                    // free-form, not sensitive
    Secret,                                  // password input + stored via vault
    Number,
    Bool,
    Url,                                     // scheme http/https validated
    Select { options: Vec<SelectOption> },   // fixed dropdown
}
```

`Secret` fields go into the encrypted vault rather than plain config.
Everything else is stored as the connector / rule expects.

## The blueprint

What `apply_recipe` actually does when you click Deploy:

```rust
struct RecipeBlueprint {
    connector_configs: Vec<ConnectorConfigStep>,   // upsert connector configs
    rules:             Vec<RuleStep>,               // create rules
    ai_config:         Option<AiConfigStep>,        // set AI adapter
    summary:           Option<String>,              // plain-language preview
}
```

Every string in the blueprint may contain `${input_id}` placeholders.
At apply time the runtime substitutes them from the user's filled
inputs:

```
User fills:                  Blueprint says:                  After substitute:
─────────────────────────    ──────────────────────────────   ──────────────────────────────
telegram_token = "abc123"    { bot_token: "${telegram_token}"} { bot_token: "abc123"}
welcome_text = "hi friend"   text = "${welcome_text}"          text = "hi friend"
```

Substitution rules:

- **JSON values** — every string leaf is template-substituted; non-
  string leaves (numbers, bools, arrays, objects) pass through
  unchanged.
- **TOML strings** — substituted as plain template strings before
  parse.
- **Unknown placeholders** surface as `ApplyError::UnknownPlaceholder`
  **before any side effect**. The pre-flight is atomic-ish — nothing
  is written when substitution fails.

## The apply flow

```
   User clicks Apply on a recipe in the library
             │
             ▼
   POST /recipes/{id}/preflight  ─►  preflight checks:
             │                        - all Required filled?
             │                        - all placeholders bound?
             │                        - capabilities available?
             │                        - any blocking precondition?
             ▼
   POST /recipes/{id}/preview     ─►  render summary, show diff
             │
             ▼
   POST /recipes/{id}/apply       ─►  substitute → upsert connectors
             │                        → create rules → set AI adapter
             │                        → return ApplyReport
             ▼
   ApplyReport { connectors_upserted, rules_created, … }
```

Preflight is cheap and idempotent — call it as the user types if you
want live "ready to deploy" feedback. Apply has side effects and
returns once they've all landed.

## Library browsing

Server-side filtering, every time:

```rust
struct RecipeFilter {
    query:           Option<String>,              // fuzzy match on name/desc/connectors
    category:        Option<RecipeCategory>,
    tags:            Vec<String>,                  // AND across tags
    sources:         Vec<RecipeSourceFilter>,      // builtin, user, community
    favorites_only:  bool,
    limit:           Option<usize>,                // capped at 100 server-side
    sort:            RecipeSort,                   // Recommended | Newest | Alphabetical
}
```

Frontends pass a filter, get a slice back. They **never** hold the
full catalogue and never filter in-memory. Same shape feeds desktop
IPC, dashboard HTTP, and any future surface.

The dashboard's `recipes/RecipeLibraryOverlay.tsx` (or the desktop
equivalent) renders this overlay. RecipeCard / RecipeQuickView render
individual entries.

## User recipes — current status

User-saved recipes are W2.B work, partially landed:

- The wire shape is in place — `RecipeSource::User`, the
  `/recipes/user` POST endpoint, the `/recipes/user/{id}` DELETE
  endpoint, the authoring module under
  `crates/springtale-runtime/src/operations/recipes/authoring.rs`.
- The SQLite `recipes_user` table **does not yet ship**. The
  `library::load_user_recipes()` function returns
  `Ok(Vec::new())` until the table is added.
- Until the storage lands, calls to `/recipes/user` will fail
  cleanly with an `OperationError::NotSupported`.

This is a known limitation tracked in
[`docs/arch/AUDIT-NOTES.md`](../arch/AUDIT-NOTES.md). The current
release ships built-in recipes only.

## Community recipes — future

`RecipeSource::Community { author, signature }` is wire-shaped now
but inactive. The plan (W3.A):

1. Community publishes a recipe + Ed25519 signature.
2. User pulls it (via Veilid in Phase 3, or HTTP-fetch + manifest
   signature in the meantime).
3. Sentinel verifies the signature against the trusted-authors list
   (same trust model as community WASM connectors — see
   [`adding-a-connector.md`](../contributing/adding-a-connector.md)).
4. UI renders the trust badge (Builtin > User > Community-trusted >
   Community-unknown).

Not active in the current release.

## API surface

16 endpoints under `/recipes/*`. Full list:
[`reference/api.md §3.15`](../reference/api.md). Recipe data shape
reference: [`reference/recipes-format.md`](../reference/recipes-format.md).
Tutorial: [`tutorials/04-deploy-recipe.md`](../tutorials/04-deploy-recipe.md).
Authoring walkthrough: [`cookbook/authoring-a-recipe.md`](../cookbook/authoring-a-recipe.md).

## Architecture invariants

Two rules the recipes subsystem holds across surfaces:

1. **Backend owns the decisions.** Every is-this-required vs
   optional vs advanced vs baked decision is in the `Recipe` value
   the backend returns. Frontends render what they're told; they
   don't invent categories, classify fields, or filter in-memory.
   Same `Recipe` shape feeds desktop IPC, dashboard HTTP, future
   frontends. See `feedback_thin_frontend_modular_backend` /
   `feedback_zero_frontend_logic`.
2. **All decisions are author-declared, never inferred.** The author
   of a recipe explicitly classifies every input field. No
   "auto-classify" heuristic. Matches the IFTTT / Zapier / GitHub
   Actions model.

## W-series milestone tracking

The recipes UX work is tracked under W-series milestones (parallel
to the G-series that brought disguise tray, quick-hide, gossip bus,
hot-reload, Python bindings, etc.):

| Milestone | What it is | Status |
|---|---|---|
| W1.C | Progressive-disclosure deploy form (Required → Optional → Advanced) | Shipped |
| W1.D | Preflight + Deploy-button gating on Required fields | Shipped |
| W1.F | Approval-gate UX dispatcher (Tauri side; bridges `ChannelApprovalGate`) | Shipped |
| W2.B | User-recipe authoring (UI + storage) | Partial — UI shipped, `recipes_user` table pending |
| W3.A | Community-recipe signature verification + trust badges | Wire-shape only |

## Gotchas

- **Apply is not transactional across steps.** A blueprint that
  upserts 3 connectors and creates 2 rules may leave you with all
  3 connectors but only the first rule if the second rule fails
  parse. The runtime logs the partial state in `ApplyReport`; a
  proper rollback path is future work.
- **Computed inputs aren't supported yet.** A recipe can't say
  "derive `${url_host}` from `${url}`" — every placeholder must
  correspond to a declared input. Future work adds a `DerivedInput`
  enum.
- **Built-in recipes ship in the binary.** Adding one requires
  changing `crates/springtale-runtime/src/operations/recipes/builtin.rs`
  and rebuilding. User authoring (W2.B) is the path for non-built-in
  recipes once the storage table lands.
- **Difficulty is author-set**, not measured. A recipe author can
  ship a Quick recipe that turns out to require fiddling. Report
  the mismatch as a bug against the recipe.
