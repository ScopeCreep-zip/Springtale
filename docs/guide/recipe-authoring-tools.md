# Recipe authoring tools

When you're building or editing a recipe, four tools in the desktop
help you ship something that actually works:

| Tool | What it does | When you use it |
|---|---|---|
| **Preflight** (W1.D) | Validates everything a recipe needs before Deploy | Always — runs continuously as you fill the form |
| **Preview / dry-run** (W2.C) | Renders what the recipe would do, no side effects | Before clicking Deploy — sanity check the chain |
| **Test This Step** (W2.C / Phase C) | Fires the chain in DryRun mode up to one step | Debugging a specific step's output |
| **Selector picker** | Opens a webview, lets you click an element, returns the CSS selector | Authoring `Extract::Css` schemas for web recipes |

These are authoring-time tools. None of them touches the live
runtime — preview and test-step run a separate, throwaway engine;
preflight queries state but doesn't change it; the selector picker
is purely a UI helper.

## Preflight (W1.D)

> The worst UX is a bot that "deploys" but silently does nothing.

Preflight is the live checklist that runs as the user fills the
deploy form. It validates every prerequisite — required inputs,
input formats, connector availability, AI config — and surfaces
each one with a typed fix-hint.

```
┌────────────────────────────────────────────────────────┐
│  Deploy form                              [Preflight]  │
│                                                        │
│  Bot token        [********]              ✓ Verified   │
│  Target chat id   [           ]           ✗ Blocking    │
│                   → Fill in the Telegram chat id.      │
│  AI adapter       Anthropic ▼             ⏳ Pending    │
│                   → Provider is configured. Test       │
│                     connection with the AI panel.      │
│  Refresh interval [15] minutes            ⚠ Warning     │
│                   → Below 5 minutes may rate-limit.    │
│                                                        │
│  [ Deploy ]  (disabled until 0 Blocking items)         │
└────────────────────────────────────────────────────────┘
```

### Statuses

```rust
pub enum PreflightStatus {
    Blocking,   // must fix before deploy
    Warning,    // safe to deploy, but here's something to know
    Verified,   // we checked, it's good
    Pending,    // we're still checking
}

pub struct PreflightItem {
    pub label:   String,
    pub status:  PreflightStatus,
    pub fix:     Option<PreflightFix>,   // typed UI hint
}
```

Source: `crates/springtale-runtime/src/operations/preflight/types.rs`.

The frontend renders the report and decides whether to enable
Deploy based on the report's `deployable` boolean — **never**
re-deriving it. Every classification is in Rust. This is the
architecture invariant from `feedback_zero_frontend_logic`: the
backend owns the decision; frontends render what they're told.

### What gets checked

Each check is a pluggable function in
`crates/springtale-runtime/src/operations/preflight/checks.rs`:

| Check | What it validates |
|---|---|
| `required_inputs_filled` | Every `FieldVisibility::Required` field has a non-empty value |
| `input_format` | Per-field type validation (url is http/https, secret isn't blank, etc.) |
| `connector_loaded` | Every connector in `recipe.connectors_used` is installed and enabled |
| `connector_capability` | The connector has the capabilities the recipe needs |
| `ai_config` | If `recipe.ai_required` and an AI step is in the blueprint, an AI adapter is configured |
| `ai_structured_outputs` | If a step uses `Extract::LlmSchema`, the AI adapter supports structured outputs |
| `cron_sane` | Cron expressions parse and the schedule isn't pathological |
| `host_allowlist` | URLs in `Extract::Css` / browser steps are on the connector's host allow-list |

On top of these, preflight and apply validate every `RunConnector`
step's `params` against the connector's action schema. A param that is
a whole `${…}` placeholder is typed by the schema; a wrong field name or
a nonexistent action is refused before anything is created.

Adding a new check is a Rust patch — see
[`docs/contributing/extension-points.md`](../contributing/extension-points.md).

### Engine + report

```rust
pub fn preflight_recipe(
    runtime: &RuntimeState,
    recipe_id: &str,
    inputs: &RecipeInputs,
) -> PreflightReport
```

The engine orchestrates checks, aggregates the result, returns a
`PreflightReport` with a `deployable: bool` plus the per-check
items. Cheap and idempotent — call it as the user types.

### Tauri / API surface

The desktop calls `springtale_runtime::operations::preflight::preflight_recipe`
through a Tauri command. HTTP exposure is planned but not shipped
in the current release.

## Preview / dry-run (W2.C)

The "what would happen if I deployed this" view. Substitutes the
user's inputs into each rule's TOML, parses it, fires a synthetic
trigger, evaluates against an in-memory `RuleEngine`, and returns
plain-language narrative steps the frontend renders as a comic
strip.

**No side effects** — the engine is constructed fresh and dropped
when the function returns. Preview is safe to call anywhere:
from the deploy form's "Preview" button, from the recipe authoring
Clear Check, from a sandbox endpoint.

```rust
// crates/springtale-runtime/src/operations/preview.rs
pub fn preview_recipe(
    recipe_id: &str,
    inputs: &RecipeInputs,
) -> Result<PreviewReport, PreviewError>

pub struct PreviewReport {
    pub steps: Vec<PreviewStep>,
}

pub struct PreviewStep {
    pub action_kind: String,
    pub summary:     String,           // plain-language
    pub params:      serde_json::Value, // what would dispatch
}
```

Action dispatch isn't simulated — the action discriminant + params
are surfaced verbatim so the user sees what would run. Mocking a
connector dispatcher is a planned follow-up; for click-and-play
UX, "would run X with Y" is enough.

### The comic-strip render

The frontend renders each `PreviewStep` as a panel. For a
3-step recipe (HTTP get → Extract → Telegram send), the preview
looks like:

```
┌───────────────┐  ┌───────────────┐  ┌───────────────┐
│ 1. HTTP get   │  │ 2. Extract    │  │ 3. Telegram   │
│                │  │   (Feed)      │  │   send        │
│ GET            │  │                │  │                │
│ https://...    │  │ Find entries   │  │ Chat: 12345    │
│                │  │ in body        │  │ Text:          │
│ Returns: bytes │  │                │  │ "${item.title}"│
│                │  │ Returns:       │  │                │
│                │  │ {entries: [..]}│  │                │
└───────────────┘  └───────────────┘  └───────────────┘
```

Useful for catching "wait, why does it post the URL twice?" before
deploy.

### Preview vs. Apply

| | Preview | Apply |
|---|---|---|
| Side effects | None | Real — installs the recipe |
| Engine | Throwaway, in-memory | Live runtime |
| Trigger | Synthetic | Real — wired to the rule's trigger source |
| Substitution | Yes | Yes |
| Dispatch | Surfaced verbatim, not run | Run for real |

Preview is the **safe** version of Apply. Always preview first when
you're authoring; the deploy form makes Preview one click away from
Deploy.

## Test This Step (W2.C / Phase C)

Single-step debug. You're on a recipe with 5 steps; step 3
("Extract") is returning weird output. You want to see what's
going into step 3 and what's coming out — without firing the
whole chain through to the Telegram send at step 5.

```
┌──────────────────────────────────────────────────────┐
│  Recipe deploy form                                  │
│                                                      │
│  Rule 1, Step 1: HTTP get      [Test This Step]      │
│  Rule 1, Step 2: Transform     [Test This Step]      │
│  Rule 1, Step 3: Extract       [Test This Step]  ◄── │
│  Rule 1, Step 4: ForEach       [Test This Step]      │
│  Rule 1, Step 5: Telegram send [Test This Step]      │
│                                                      │
│  [ Deploy ]                                          │
└──────────────────────────────────────────────────────┘
```

Click "Test This Step" on step 3:

1. The chain fires from step 1 through step 3, in `DryRun` mode.
2. Read-only steps run for real (HTTP get hits the real URL,
   AiComplete posts a real prompt, Extract parses real bytes).
3. Side-effecting steps are stubbed (no actual Telegram send,
   no real file write, no dedupe state mutation).
4. The dispatcher returns the recorded `StepOutput` for step 3.
5. The UI renders the input + output + any error.

### Why dispatch up to N, not just N

Producing realistic output for step N usually requires the
upstream `last_*_output` aliases populated by steps 1..N-1. The
simplest correct path is to run the chain in DryRun mode and stop
after N.

`v1` always re-fires from step 1. n8n's pinned-data pattern (skip
steps 1..N-1, seed `last_*` from cached values) is a Phase C+
enhancement.

### DryRun mode

```rust
pub enum ExecutionMode {
    Normal,    // real dispatch, persists to executions log
    DryRun,    // side-effecting arms stubbed, read arms real
}
```

In DryRun:

- HTTP get, AiComplete, Extract, Transform, Delay → real.
- Connector send_message / write_file / RunShell → stubbed.
- `Action::Dedupe` → never inserts into `dedupe_seen`; reports
  "would have suppressed" or "would have stored".
- Executions log records the row with `mode = "dry_run"` so it's
  distinguishable; same retention as normal rows.

### Privacy

The result is **not** persisted to the executions log under the
rule's normal stream — the dispatcher records the run with
`mode = "dry_run"` and the default 14-day retention sweeps it like
any other row. The recorded row has `summary_bytes` like any
normal row; the content isn't captured.

### Surface

`runtime::operations::test_step::test_recipe_step(recipe_id, inputs, rule_index, step_index)`.
Tauri command: `test_recipe_step`. UI: `TestStepButton.tsx` per
step in the deploy form.

## Selector picker

For recipes that use `Extract::Css` against a web page, you need
the right CSS selectors. Authoring those by reading the page
source is tedious. The selector picker opens the target URL in a
Tauri webview with a click-to-pick overlay:

```
┌──────────────────────────────────────────────────────────┐
│  https://news.example.com/                               │
│  ───────────────────────────────                         │
│                                                          │
│    [Logo]   Search: [          ]   Profile               │
│                                                          │
│   ┌─────────────────────────────────────────────┐        │
│   │ ◄ HIGHLIGHTED — h1.article-title             │        │
│   │ Today's top story                            │ click │
│   └─────────────────────────────────────────────┘        │
│                                                          │
│   Subhead text...                                        │
│                                                          │
│  ┌──────────────────────────────────────────┐            │
│  │  Picked: h1.article-title  [ Confirm ]   │            │
│  └──────────────────────────────────────────┘            │
└──────────────────────────────────────────────────────────┘
```

What it does:

1. Tauri opens a new webview pointing at the recipe's target URL.
2. Injects a bundled `picker.js` overlay that highlights elements
   on hover and emits a `selector-picked` event on click.
3. The host listens for the event, returns the chosen selector
   to the caller.
4. Recipe form receives the selector string and substitutes it
   into the field.

**No chromiumoxide / headless browser.** This is an authoring-time
tool using Tauri's own webview — when you need a headless
browser for an actual scrape, use `connector-browser` from the
recipe blueprint.

### Privacy

The webview navigates to the user-supplied URL using the desktop's
own networking stack. Constraints:

- **Fixed URL** — no user-typed address-bar navigation. The
  picker opens at the URL the recipe requires.
- **Browser features disabled** — no microphone, no geolocation,
  no permission prompts.
- **`host_allowlist`** — picker.js checks the URL's host before
  binding the highlight handler. Advisory only; the user could
  still navigate the webview manually, but the picker doesn't
  return selectors from outside the allowed list.
- **Cancel via Escape or close button** — no required interaction.

The picker doesn't capture screenshots, doesn't record clicks
other than the selector emit, doesn't persist any state beyond the
returned selector string.

### Surface

Tauri command: `pick_selector` (sync — opens the window, waits
for the user's pick or cancel, returns the selector or `None`).
UI entry: `SelectorPickerOverlay.tsx`.

## Tying it together — the authoring flow

```
   User wants to author a new page-change-watcher recipe
                            │
                            ▼
        Fill the deploy form's required inputs
                            │
                            ▼
      Click "Pick selector" → selector_picker opens
       webview → user clicks h1 → returns "h1.title"
                            │
                            ▼
      Selector lands in Extract::Css schema field
                            │
                            ▼
       Preflight re-runs → "all blocking items
         cleared" → Deploy button enables
                            │
                            ▼
        Click "Preview" → see the comic-strip
        rendering of what would happen
                            │
                            ▼
      "Test This Step" on step 3 (Extract) → see
       the real fetched bytes parsed against the
        CSS schema, verify output looks right
                            │
                            ▼
                       Click Deploy
                            │
                            ▼
        Apply runs against the live runtime,
        recipe is now an installed rule
```

## Gotchas

- **Preflight is per-input change.** Don't expect a deploy
  attempt to surface a check the form already cleared — the
  status is what the engine reported at the last evaluation.
- **Preview action dispatch is fake.** A Preview that shows
  `Telegram send: chat_id 12345, text "hello"` means **the
  preview won't actually send it**, but a real Deploy will.
- **Test This Step writes a `dry_run` execution row.** The
  drift detector filters these by default; the executions list
  shows them with a "DRY" badge. The 14-day retention applies.
- **Selector picker requires the page to load.** Pages that
  Cloudflare-challenge or require login render the challenge in
  the webview; you'll need to either complete the challenge in
  the picker (it persists the session for that webview only) or
  give up and craft the selector by hand.
- **`pick_selector` is single-pick.** Picking multiple selectors
  (one per field in a Css schema) means re-opening the picker per
  field. Multi-pick UX is a future enhancement.

## See also

- [`docs/guide/recipes.md`](recipes.md) — the recipes concept
- [`docs/guide/dedupe-and-extract.md`](dedupe-and-extract.md) —
  `Action::Extract` modes (Css, JsonPath, Readability, Feed, Ical,
  LlmSchema, Passthrough)
- [`docs/guide/executions-and-drift.md`](executions-and-drift.md) —
  what shows up in the log after a Test or Preview
- [`docs/reference/recipes-format.md`](../reference/recipes-format.md) —
  recipe schema reference
- `crates/springtale-runtime/src/operations/preflight/`
- `crates/springtale-runtime/src/operations/preview.rs`
- `crates/springtale-runtime/src/operations/test_step.rs`
- `tauri/apps/desktop/src-tauri/src/commands/selector_picker.rs`
