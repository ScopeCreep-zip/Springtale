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
      ┌─────────┬──────────┐
      ▼         ▼          ▼
   Webhooks  Dashboard  (SPA)
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

### 3.4 Formations

Formations are cooperating groups of agents with a shared intent. See [`docs/intended-arch/COOPERATION.md`](../intended-arch/COOPERATION.md).

| Method | Path | Description |
|---|---|---|
| GET | `/formations` | List all formations with member count, intent, momentum tier |
| POST | `/formations` | Create a formation |
| GET | `/formations/intents` | List available intent templates (Reconnoiter / Execute / Stabilize / Surge / Dissolve) |
| POST | `/formations/deploy-team` | Deploy a multi-agent team in one call |
| POST | `/formations/{id}/deploy` | Move formation to deployed state |
| POST | `/formations/{id}/pause` | Pause (members stop acting on cadence ticks) |
| POST | `/formations/{id}/resume` | Resume from paused |
| POST | `/formations/{id}/dissolve` | Dissolve team, stop all members |
| PUT | `/formations/{id}/intent` | Change intent |
| POST | `/formations/{id}/members` | Add a member |
| POST | `/formations/{id}/cycle-intent` | Cycle to the next intent template |
| POST | `/formations/{id}/cycle-autonomy` | Cycle autonomy level of all members |
| POST | `/formations/{id}/toggle-guard` | Toggle the formation guard rails |

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
| POST | `/canvas/update` | Apply a `CanvasUpdate` (drag, reposition, re-wire) |
| GET | `/canvas/stream` | SSE stream of canvas updates. Accepts `?token=...` |

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
| PUT | `/safety` | Update safety config (window title, auto-lock, content protection) |

### 3.13 Data

| Method | Path | Description |
|---|---|---|
| POST | `/data/export` | Export all user data. Optional encryption with vault passphrase. |

### 3.14 Webhooks

| Method | Path | Description |
|---|---|---|
| POST | `/webhook/{connector}/{trigger}` | Inbound webhook, forwarded to the connector's trigger handler after token auth |

The endpoint requires the bearer token like every other authenticated route. External senders need the token in the `Authorization` header. In addition, each connector performs its own webhook signature verification on the body via `Connector::verify_webhook()` — HMAC-SHA256 for GitHub, RSA for Kick, and so on.

### 3.15 Dashboard

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
