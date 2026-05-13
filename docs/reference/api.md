# Management API Reference

`springtaled` exposes a REST API over HTTP for connector management, rule CRUD, formation orchestration, event streaming, canvas updates, configuration, and webhook ingestion.

## 1. Overview

- **Default bind:** `127.0.0.1:8080` (configurable via `[api] bind`)
- **Transport:** HTTP (use a reverse proxy for remote TLS termination)
- **Content type:** `application/json`
- **Live streams:** Server-Sent Events on `/events/stream` and `/canvas/stream`

```
  Client                       springtaled
       │                            │
       │  HTTP request              │
       │  Authorization: Bearer X   │
       ├───────────────────────────>│
       │                            │
       │                     ┌──────┴────────────────────┐
       │                     │  Middleware Stack         │
       │                     │                           │
       │                     │  1. ValidatedPath (≤256B) │
       │                     │  2. require_auth          │
       │                     │  3. RateLimit (100 req/s) │
       │                     │  4. Buffer                │
       │                     │  5. BodyLimit (1 MiB)     │
       │                     │  6. Timeout (30 s)        │
       │                     │  7. CSP / X-Frame-DENY    │
       │                     └──────┬────────────────────┘
       │                            │
       │  JSON response             │
       │<───────────────────────────┤
```

*Fig. 1. Request flow through the middleware stack.*

---

## 2. Authentication

Every route except `/health`, `/ready`, and `/ui` requires a bearer token. Webhook routes (`/webhook/{connector}/{trigger}`) also require the token — the connector then performs its own signature verification on the body (HMAC-SHA256 for GitHub, RSA for Kick, etc.) inside its `verify_webhook()` implementation.

The token is derived from the vault passphrase:

```
token = hex(HMAC-SHA256(passphrase, "springtale-api-token"))
```

Verification uses constant-time comparison (`subtle::ConstantTimeEq`). There is no separate API key — rotating the token rotates the passphrase.

Authenticated routes also go through a CSRF-protection middleware
(`require_csrf_protection`) that rejects cross-origin requests with
unsafe methods. SSE readers are exempt because the browser `EventSource`
cannot originate a state-changing request.

```bash
# Typical client
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8080/connectors
```

**SSE fallback.** `EventSource` cannot set custom headers, so `/events/stream` and `/canvas/stream` accept `?token=...` as a query parameter. This is safe because the daemon binds loopback only; the token is already tied to the passphrase.

---

## 3. Endpoint Catalogue

```
                           springtaled HTTP API
                                    │
      ┌─────────┬──────────┬────────┼────────┬──────────┬──────────┐
      ▼         ▼          ▼        ▼        ▼          ▼          ▼
   Health   Connectors  Rules  Formations  Canvas   Events      Config
                                                     (SSE)
      ┌─────────┬──────────┬────────┼────────┬──────────┬──────────┐
      ▼         ▼          ▼        ▼        ▼          ▼          ▼
   Agents   Authors   Bot admin Sessions   Memory    Safety       Data
      ┌─────────┬──────────┬──────────┬──────────┬─────────────┐
      ▼         ▼          ▼          ▼          ▼             ▼
   Send    Diagnostics  Fixes    Onboarding  Templates     Webhooks
                                                                 │
                                                                 ▼
                                                          Dashboard (SPA)
```

*Fig. 3. Route groups at a glance. Public routes: health, ready, `/ui`, `/ui/*`. Everything else requires the bearer token.*

### 3.1 Health

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/health` | — | Liveness. Returns `{"status":"ok"}`. |
| GET | `/ready` | — | Readiness. `200` when booted, `503` while booting. |

### 3.2 Connectors

| Method | Path | Description |
|---|---|---|
| GET | `/connectors` | List installed connectors with enabled state |
| GET | `/connectors/schemas` | JSON Schema for every connector's config |
| GET | `/connectors/available` | Built-in + community connectors discoverable for install |
| POST | `/connectors/setup` | Interactive setup wizard — validates config |
| POST | `/connectors/install` | Install from manifest. Verifies Ed25519 signature before registering |
| DELETE | `/connectors/{name}` | Remove a connector. Leaves rules intact. |
| DELETE | `/connectors/{name}/cascade` | Remove connector **and** any rules that reference it |
| GET | `/connectors/{name}/config` | Stored config (secrets redacted) |
| POST | `/connectors/{name}/upsert-config` | Update connector config atomically |
| GET | `/connectors/{name}/outputs` | Recent action outputs (cap 100, oldest dropped) |
| POST | `/connectors/{name}/enable` | Enable a disabled connector |
| POST | `/connectors/{name}/disable` | Disable without removing |
| POST | `/connectors/{name}/reload` | G4 — hot-reload a connector's running instance against the latest stored config without restarting the daemon |
| POST | `/connectors/{name}/test` | Dry-run an action with synthetic input |

### 3.3 Rules

| Method | Path | Description |
|---|---|---|
| GET | `/rules` | List all rules with status + trigger type |
| POST | `/rules` | Create a rule from JSON |
| POST | `/rules/parse` | Natural-language → Rule via the configured AI adapter |
| GET | `/rules/schema` | JSON Schema for the `Rule` type |
| PUT | `/rules/{id}` | Replace a rule |
| DELETE | `/rules/{id}` | Delete a rule |
| POST | `/rules/{id}/toggle` | Enable ↔ disable |
| POST | `/rules/{id}/run` | Dry-run against a synthetic trigger event |
| POST | `/rules/{id}/reassign` | Reassign a rule to a different connector or agent |
| POST | `/rules/connector` | Create a rule for a connector event (convenience wrapper) |
| GET | `/rules/connector/{name}` | List rules triggered by a specific connector |

### 3.4 Formations & cooperation streams

Formations are cooperating groups of agents with a shared intent. See [`docs/guide/cooperation.md`](../guide/cooperation.md). Every formation command pushes onto the bot's `FormationCommand` channel; the bot is the only code path that materialises live `Formation` structs from DB rows.

| Method | Path | Description |
|---|---|---|
| GET | `/formations` | List all formations with member count, intent, momentum tier |
| POST | `/formations` | Create a formation |
| GET | `/formations/{id}` | Live formation detail (momentum, rally tokens, attention load, guard status, member health + liveness) via `LiveFormationReader` |
| GET | `/formations/{id}/commands` | 3×3 colony-canvas command grid for this formation, with status-aware enable flags. Used by the dashboard formation detail card. |
| GET | `/formations/{id}/members/eligible` | Eligible-for-removal member list for the formation member overlay. |
| GET | `/formations/intents` | List available intent templates (Reconnoiter / Execute / Stabilize / Surge / Dissolve) |
| POST | `/formations/deploy-team` | Deploy a multi-agent team in one call |
| POST | `/formations/{id}/deploy` | Move formation to deployed state |
| POST | `/formations/{id}/pause` | Pause (members stop acting on cadence ticks) |
| POST | `/formations/{id}/resume` | Resume from paused |
| POST | `/formations/{id}/dissolve` | Dissolve, persist mental model first, stop all members |
| POST | `/formations/{id}/rally` | Manual rally — consume a rally token around the weakest member |
| PUT | `/formations/{id}/intent` | Change intent |
| POST | `/formations/{id}/members` | Add a member (by connector name) |
| DELETE | `/formations/{id}/members` | Remove a member (by connector name) |
| POST | `/formations/{id}/cycle-intent` | Cycle to the next intent template |
| POST | `/formations/{id}/cycle-autonomy` | Cycle autonomy level of all members (observe → suggest → approve → autonomous → observe) |
| POST | `/formations/{id}/toggle-guard` | Toggle the formation guard rails |
| GET | `/cooperation/events` | SSE stream of formation lifecycle, momentum transitions, rally events, and interference events. Accepts `?token=...` and optional `?formation_id=...` filter. The dashboard's live cooperation overlay consumes this. |

### 3.5 Agents

Autonomy levels: `observe → suggest → act-with-approval → act-autonomously`.

| Method | Path | Description |
|---|---|---|
| GET | `/agents/states` | All agents with current autonomy and formation membership |
| GET | `/agents/{name}/autonomy` | Current autonomy level for one agent |
| PUT | `/agents/{name}/autonomy` | Set autonomy level |
| POST | `/agents/{name}/autonomy/step` | Step one level toward more autonomous |

### 3.6 Canvas

The colony canvas is a live pixel-art visualisation of connectors, rules, agents, and formations.

| Method | Path | Description |
|---|---|---|
| GET | `/canvas` | Full canvas state (nodes + edges) |
| GET | `/canvas/connections` | Just the edge metadata |
| GET | `/canvas/stream` | SSE stream of canvas updates. Accepts `?token=...` |

> **Note:** there is no `POST /canvas/update` HTTP endpoint. Canvas layout
> changes (drag, reposition, re-wire) happen in the Tauri desktop via the
> dashboard's IPC layer, which writes directly to the dashboard's local
> layout state. The daemon-side `CanvasState` is read-only over HTTP.

### 3.7 Events

| Method | Path | Query params | Description |
|---|---|---|---|
| GET | `/events` | `limit`, `offset`, `connector` | Paginated event log |
| GET | `/events/stream` | `?token=...` | SSE stream of new events |

### 3.8 Configuration

| Method | Path | Description |
|---|---|---|
| GET | `/config` | List all config keys |
| GET | `/config/{key}` | Get a single config value |
| PUT | `/config/{key}` | Set a single config value |
| POST | `/config/ai` | Select AI adapter (`noop`, `ollama`, `openai`, `anthropic`) — hot-swaps at runtime |
| POST | `/config/ai/configure` | Adapter-specific settings (endpoint, model, API key) |
| POST | `/config/connector/{name}` | Store encrypted connector config |
| GET | `/config/heartbeat` | Heartbeat interval (seconds) |
| PUT | `/config/heartbeat` | Update heartbeat interval |

### 3.9 Trusted Authors

Author Ed25519 public keys used to verify signed manifests.

| Method | Path | Description |
|---|---|---|
| GET | `/authors` | List registered author keys |
| POST | `/authors/{name}` | Register a new author key |
| DELETE | `/authors/{name}` | Remove an author |

### 3.10 Bot

| Method | Path | Description |
|---|---|---|
| GET | `/bot/status` | Bot connection status for each chat connector |
| GET | `/bot/formations` | Active bot formations |
| GET | `/bot/memory` | Memory session statistics |

### 3.11 Sessions & Memory

| Method | Path | Description |
|---|---|---|
| GET | `/sessions` | Active agent sessions |
| POST | `/memory/audit` | Inspect memory session counts |
| POST | `/memory/compact` | Delete oldest entries beyond per-session limit |

### 3.12 Safety

| Method | Path | Description |
|---|---|---|
| GET | `/safety` | Current sentinel / behavioural monitor config |
| PUT | `/safety` | Update full safety config (window title, auto-lock, content protection, disguise profile, quick-hide shortcut) |
| POST | `/safety/disguise/active` | G5d — flip just the disguise-active flag. Focused endpoint that avoids the lost-update race two tabs would hit on the full-config PUT path. Body: `{ "active": bool }` |
| POST | `/safety/disguise/profile` | G5f — switch the disguise icon profile (one of `calculator`, `files`, `notes`, `springtale`). Body: `{ "profile": "<id>" }`. Tray icon is swapped at runtime |
| POST | `/safety/panic_tap_count` | Set the number of taps required on the panic gesture before the wipe fires. Body: `{ "count": u32 }` |

### 3.13 Data

| Method | Path | Description |
|---|---|---|
| POST | `/data/export` | Export all user data. Optional encryption with vault passphrase. |

> **Note:** import is **CLI-only**. `springtale-cli data import --input <path>`
> calls the runtime's `import_data()` function directly against the local
> SQLite backend. There is no HTTP endpoint because import is an offline
> operation that requires the daemon to not be writing concurrently.

### 3.14 Send

| Method | Path | Description |
|---|---|---|
| POST | `/send` | Execute an `Action` directly against a connector. Capability-checked through the sentinel, same as rule-dispatched actions. No back door. |

### 3.15 Diagnostics, Fixes, Onboarding, Templates, Recipes

These routes back the **Doctor** and **Onboarding** flows in the desktop shell.

| Method | Path | Description |
|---|---|---|
| GET | `/diagnostics` | Run the current set of runtime health checks; returns a list of issues with severity + description |
| GET | `/fixes` | List available auto-repair suggestions bound to diagnostic ids |
| GET | `/fixes/{id}` | Fetch a single fix with its proposed action |
| POST | `/fixes/{id}/apply` | Apply a fix |
| GET | `/onboarding/platforms` | List platforms that have onboarding templates (telegram, discord, github, etc.) |
| POST | `/onboarding/{platform}` | Apply an onboarding template for the given platform |
| GET | `/templates` | List rule / connector templates bundled with the daemon |
| POST | `/templates/{name}` | Write a template into the current store |
| GET | `/recipes` | List curated automation recipes (browseable cookbook surface) |
| GET | `/recipes/categories` | Recipe categories |
| GET | `/recipes/{id}` | One recipe by id |
| GET | `/recipes/{id}/pieces` | Modular pieces composing the recipe |
| GET | `/recipes/{id}/export` | Export the recipe as TOML |
| POST | `/recipes/{id}/favorite` | Toggle favorite |
| POST | `/recipes/{id}/recent` | Mark recently viewed |
| POST | `/recipes/{id}/apply` | Apply the recipe — installs rules / connectors as defined |
| POST | `/recipes/{id}/render` | Render the recipe against current state (preview-friendly) |
| POST | `/recipes/{id}/preflight` | Check preconditions (capability grants, connector availability) before apply |
| POST | `/recipes/{id}/preview` | Dry-run preview of what apply would do |
| POST | `/recipes/{id}/fork` | Fork a recipe into a user-saved variant |
| POST | `/recipes/user` | Save a custom user recipe |
| DELETE | `/recipes/user/{id}` | Delete a user-saved recipe |
| POST | `/recipes/import` | Import a recipe from TOML |

### 3.16 Webhooks

| Method | Path | Description |
|---|---|---|
| POST | `/webhook/{connector}/{trigger}` | Inbound webhook, forwarded to the connector's trigger handler after token auth |

The endpoint requires the bearer token like every other authenticated route. External senders need the token in the `Authorization` header. In addition, each connector performs its own webhook signature verification on the body via `Connector::verify_webhook()` — HMAC-SHA256 for GitHub, RSA for Kick, and so on.

### 3.17 Dashboard

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/ui` | — | Embedded SPA index |
| GET | `/ui/{*path}` | — | SPA static assets |

---

## 4. Middleware Stack

```
  ┌────────────────────────────────────────────────────────────────┐
  │                      Middleware Stack                          │
  │                                                                │
  │   1. TraceLayer                    HTTP trace span             │
  │   2. SetResponseHeaderLayer × 5    security headers (§4.1)     │
  │   3. RequestBodyLimitLayer         1 MiB per request           │
  │   4. HandleErrorLayer              maps rate-limit err → 429   │
  │   5. BufferLayer (256)             fronts the rate limiter     │
  │   6. RateLimitLayer                100 req/s (configurable)    │
  │   7. TimeoutLayer                  30 s per request → 503      │
  │                                                                │
  │   require_auth middleware          Bearer header or ?token=    │
  │   ValidatedPath extractor          path segments ≤ 256 bytes   │
  │                                                                │
  └────────────────────────────────────────────────────────────────┘
```

*Fig. 2. Applied to all routes.*

### 4.1 Security Response Headers

Five headers are set on every response by `SetResponseHeaderLayer`:

| Header | Value |
|---|---|
| `X-Frame-Options` | `DENY` |
| `Content-Security-Policy` | `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' http://127.0.0.1:*; img-src 'self' data:; frame-ancestors 'none'` |
| `X-Content-Type-Options` | `nosniff` |
| `Referrer-Policy` | `no-referrer` |
| `Permissions-Policy` | `camera=(), microphone=(), geolocation=(), accelerometer=(), gyroscope=()` |

**HSTS is deliberately omitted.** RFC 6797 §8.1 requires that `Strict-Transport-Security` not be sent over plain HTTP, and `springtaled` binds `127.0.0.1` without TLS by default. An operator terminating TLS in front of the daemon (reverse proxy, mesh sidecar) should set HSTS at that layer.

---

## 5. Error Format

All errors return JSON:

```json
{
  "error": "connector not found: connector-foo"
}
```

**TABLE I. STATUS CODES**

| Code | Meaning |
|---|---|
| 200 | Success |
| 400 | Bad request (invalid JSON, missing fields, validation failure) |
| 401 | Unauthorized (missing or invalid bearer token) |
| 403 | Capability denied (manifest lacks required permission) |
| 404 | Not found |
| 409 | Conflict (name collision, toxic pair on install) |
| 413 | Payload too large (body > 1 MiB) |
| 422 | Unprocessable (JSON Schema validation failed) |
| 429 | Rate limited |
| 500 | Internal server error |
| 503 | Service unavailable (booting or degraded) |

---

## 6. Live Streams

Both SSE endpoints emit `event:` + `data:` lines. Payloads are JSON.

### 6.1 `/events/stream`

Emits every event logged to the `events` table, in order.

```
event: event
data: {"id":"...","connector":"connector-telegram","trigger_type":"message_received","timestamp":"2026-04-10T12:34:56Z"}
```

### 6.2 `/canvas/stream`

Emits `CanvasUpdate` deltas for dashboard rendering.

```
event: canvas
data: {"kind":"node_moved","id":"...","x":120,"y":80}
```

Broadcast semantics: slow consumers receive `RecvError::Lagged(n)` and must reconnect. The dashboard auto-refetches `GET /canvas` on reconnect.

---

## References

- [1] Configuration: [configuration.md](configuration.md)
- [2] CLI commands: [cli.md](cli.md)
- [3] Full architecture: [`docs/arch/ARCHITECTURE.md`](../arch/ARCHITECTURE.md) §9
- [4] Security posture: [`docs/arch/SECURITY.md`](../arch/SECURITY.md) §6
- [5] Cooperation framework: [`docs/intended-arch/COOPERATION.md`](../intended-arch/COOPERATION.md)
