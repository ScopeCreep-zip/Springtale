# Management API Reference

`springtaled` exposes a REST API for connector management, rule CRUD, event queries, and webhook ingestion.

## 1. Overview

- **Default bind:** `127.0.0.1:8080` (configurable via `[api] bind`)
- **Transport:** HTTP (no TLS on localhost — use a reverse proxy for remote access)
- **Content type:** `application/json`

```
  Client                      springtaled
  (curl, CLI,                 Daemon
   dashboard)
       │                         │
       │  HTTP request           │
       │  Authorization: Bearer  │
       │  <hmac-token>           │
       ├────────────────────────>│
       │                         │
       │                   ┌─────┴──────────────────────┐
       │                   │  Middleware Stack           │
       │                   │                            │
       │                   │  1. TraceLayer (logging)   │
       │                   │  2. RequestBodyLimit (1 MB)│
       │                   │  3. BufferLayer (256)      │
       │                   │  4. RateLimit (100 req/s)  │
       │                   │  5. Timeout (30s)          │
       │                   └─────┬──────────────────────┘
       │                         │
       │  JSON response          │
       │<────────────────────────┤
```

*Fig. 1. Request flow through the middleware stack.*

## 2. Authentication

All endpoints except `/health` and `/ready` require a bearer token.

The token is derived from the vault passphrase: `HMAC-SHA256(passphrase, "springtale-api-token")`, hex-encoded. Verification uses constant-time byte comparison.

```bash
# Example
curl -H "Authorization: Bearer $(echo -n 'springtale-api-token' | \
  openssl dgst -sha256 -hmac 'your-passphrase' -hex | cut -d' ' -f2)" \
  http://127.0.0.1:8080/connectors
```

---

## 3. Endpoints

### 3.1. Health

**`GET /health`** — Liveness probe. No auth required.

```bash
curl http://127.0.0.1:8080/health
# {"status": "ok"}
```

**`GET /ready`** — Readiness probe. No auth required. Returns `503` if booting or degraded.

```bash
curl http://127.0.0.1:8080/ready
# {"status": "ready"}    (200)
# {"status": "booting"}  (503)
# {"status": "degraded"} (503)
```

---

### 3.2. Connectors

**`GET /connectors`** — List installed connectors.

```bash
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8080/connectors
# {"connectors": [{"name": "connector-kick", "enabled": true}, ...]}
```

**`POST /connectors/install`** — Install a connector from manifest JSON.

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d @connector-kick.json \
  http://127.0.0.1:8080/connectors/install
# {"installed": "connector-kick", "version": "0.1.0"}
```

**`DELETE /connectors/{name}`** — Remove a connector.

```bash
curl -X DELETE -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:8080/connectors/connector-kick
# {"removed": "connector-kick"}
```

**`POST /connectors/{name}/enable`** — Enable a connector.

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:8080/connectors/connector-kick/enable
# {"enabled": "connector-kick"}
```

**`POST /connectors/{name}/disable`** — Disable a connector.

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:8080/connectors/connector-kick/disable
# {"disabled": "connector-kick"}
```

---

### 3.3. Rules

**`GET /rules`** — List all rules.

```bash
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8080/rules
# {"rules": [{"id": "a1b2c3d4-...", "name": "stream-announce", "status": "enabled", "trigger_type": "ConnectorEvent"}]}
```

**`POST /rules`** — Create a new rule.

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "stream-announce", "trigger": {"type": "ConnectorEvent", "connector": "connector-kick", "event": "stream_live"}, "actions": [{"type": "SendMessage", "text": "Stream is live!"}]}' \
  http://127.0.0.1:8080/rules
# {"id": "a1b2c3d4-..."}
```

**`PUT /rules/{id}`** — Update a rule (full replacement).

```bash
curl -X PUT -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{ ... updated rule JSON ... }' \
  http://127.0.0.1:8080/rules/a1b2c3d4-...
# {"updated": "a1b2c3d4-..."}
```

**`DELETE /rules/{id}`** — Delete a rule.

```bash
curl -X DELETE -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:8080/rules/a1b2c3d4-...
# {"deleted": "a1b2c3d4-..."}
```

**`POST /rules/{id}/run`** — Manual trigger (dry-run evaluation).

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:8080/rules/a1b2c3d4-.../run
# {"rule_id": "a1b2c3d4-...", "matched": true, "actions_count": 1}
```

---

### 3.4. Events

**`GET /events`** — Paginated event log.

| Query param | Type | Default | Description |
|-------------|------|---------|-------------|
| `limit` | `u32` | 50 | Max events to return |
| `offset` | `u32` | 0 | Pagination offset |
| `connector` | `String` | (all) | Filter by connector name |

```bash
curl -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:8080/events?limit=10&connector=connector-kick"
# {"events": [...], "limit": 10, "offset": 0}
```

---

### 3.5. Webhooks

**`POST /webhook/{connector}/{trigger}`** — Receive an inbound webhook.

The webhook body is forwarded to the named connector's trigger handler. Webhook signature verification (HMAC-SHA256 for GitHub, RSA for Kick) is performed by the connector, not the API layer.

```bash
# GitHub sends this automatically when configured:
curl -X POST -H "X-Hub-Signature-256: sha256=..." \
  -H "Content-Type: application/json" \
  -d '{ ... GitHub event payload ... }' \
  http://your-host:8080/webhook/connector-github/push
```

---

## 4. Middleware Stack

```
  ┌─────────────────────────────────────────────────┐
  │               Middleware Stack                   │
  │                                                  │
  │  ┌───────────────────────────────────────────┐   │
  │  │  TraceLayer                               │   │
  │  │  Logs method, path, status, duration      │   │
  │  ├───────────────────────────────────────────┤   │
  │  │  RequestBodyLimit                         │   │
  │  │  Max 1 MiB per request body               │   │
  │  ├───────────────────────────────────────────┤   │
  │  │  BufferLayer                              │   │
  │  │  256 buffered requests (backpressure)     │   │
  │  ├───────────────────────────────────────────┤   │
  │  │  RateLimit                                │   │
  │  │  Configurable req/s (default: 100)        │   │
  │  ├───────────────────────────────────────────┤   │
  │  │  Timeout                                  │   │
  │  │  30 seconds per request                   │   │
  │  └───────────────────────────────────────────┘   │
  │                                                  │
  └──────────────────────────────────────────────────┘
```

*Fig. 2. Middleware stack. Applied to all routes in order from top to bottom.*

## 5. Error Format

All errors return JSON:

```json
{
  "error": "connector not found: connector-foo"
}
```

**TABLE I. STATUS CODES**

| Code | Meaning |
|------|---------|
| 200 | Success |
| 400 | Bad request (invalid JSON, missing fields) |
| 401 | Unauthorized (missing or invalid bearer token) |
| 404 | Not found (connector, rule, or endpoint) |
| 429 | Rate limited |
| 500 | Internal server error |
| 503 | Service unavailable (daemon not ready) |

---

## References

- [1] Configuration: [configuration.md](configuration.md)
- [2] CLI commands: [cli.md](cli.md)
- [3] Connector webhook verification: [connectors/](connectors/)
