# API client: curl

The simplest way to talk to `springtaled`. Useful for ad-hoc
inspection, scripting, and as the reference any other client should
match.

## Auth

Get the bearer token:

```bash
TOKEN=$(springtale-cli auth print)
```

Or, on a remote host where the CLI isn't installed:

```bash
TOKEN=$(cat ~/.local/share/springtale/api_token)
```

The token is HMAC-SHA256 of your vault passphrase; rotated by
`springtale-cli crypto rotate-vault-key`.

## Health check (no auth)

```bash
curl http://127.0.0.1:8080/health
# {"status":"ok","version":"0.x.y"}
```

`/health` and `/ready` are the only public endpoints. Everything
else needs the bearer.

## List formations

```bash
curl -s http://127.0.0.1:8080/formations \
  -H "Authorization: Bearer $TOKEN" \
  | jq .
```

## Deploy a formation team

```bash
curl -X POST http://127.0.0.1:8080/formations/deploy-team \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "research-squad",
    "intent": "Reconnoiter",
    "guard_mode": false,
    "agents": [
      {
        "connector_name": "connector-presearch",
        "trigger_name": "search_completed",
        "action_connector": "connector-bluesky",
        "action_name": "create_post"
      }
    ]
  }'
```

## Add a connector

```bash
curl -X POST http://127.0.0.1:8080/connectors/install \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  --data-binary @- <<EOF
$(cat ./connector-mything.toml | jq -Rs .)
EOF
```

(The TOML is sent as a single JSON-encoded string in the body. There's
also a multipart endpoint if you prefer.)

## Reload a connector (G4 hot reload)

```bash
curl -X POST "http://127.0.0.1:8080/connectors/connector-mything/reload" \
  -H "Authorization: Bearer $TOKEN"
```

## Subscribe to live events via SSE

```bash
curl -N "http://127.0.0.1:8080/events/stream?token=$TOKEN"
```

SSE accepts the token via query string because some browsers don't
let you set custom headers on EventSource. The `?token=` is hashed
into the audit log and treated identically to the header form.

Each event is:

```
event: connector_event
data: {"connector":"connector-telegram","event":"message_received","payload":{...}}

event: rule_fired
data: {"rule_id":"...","name":"flag-slurs","outcome":"success"}
```

## Watch the colony canvas state

```bash
curl -N "http://127.0.0.1:8080/canvas/stream?token=$TOKEN"
```

Delta updates to the canvas as formations / connectors / rules
change.

## Watch cooperation events

```bash
curl -N "http://127.0.0.1:8080/cooperation/events?token=$TOKEN&formation_id=research-squad"
```

Formation lifecycle, momentum transitions, rally events, interference
events — the same stream the dashboard consumes for live overlays.

## Toggle a safety setting

```bash
# Activate disguise (G5d):
curl -X POST http://127.0.0.1:8080/safety/disguise/active \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"active": true}'

# Switch disguise profile (G5f):
curl -X POST http://127.0.0.1:8080/safety/disguise/profile \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"profile": "calculator"}'
```

## Export user data

```bash
curl -X POST http://127.0.0.1:8080/data/export \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"encrypt": false}' \
  > my-data.json
```

For encrypted exports, set `"encrypt": true` and the response is
binary AEAD-encrypted bytes.

## Chat with your bot in-app

```bash
# Send a message (fire-and-forget, 202 Accepted):
curl -X POST http://127.0.0.1:8080/chat \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"text": "send me the weather in Tucson every morning"}'

# Stream the bot's replies (SSE):
curl -N "http://127.0.0.1:8080/chat/stream?token=$TOKEN"
```

Each stream event's data is `{"session": "in-app", "text": "..."}`.

## Approve or deny a pending ShellExec request

```bash
# See what's waiting:
curl -s http://127.0.0.1:8080/approvals \
  -H "Authorization: Bearer $TOKEN" | jq .pending

# Land a decision:
curl -X POST "http://127.0.0.1:8080/approvals/$REQUEST_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"decision": "approve"}'
```

The requesting connector blocks until your decision lands; after the
gate's timeout (default 60s) it falls back to **deny**.

## Common patterns

### Watching the approval queue

```bash
while true; do
  COUNT=$(curl -s http://127.0.0.1:8080/approvals \
    -H "Authorization: Bearer $TOKEN" | jq '.pending | length')
  echo "$(date) — $COUNT pending"
  sleep 2
done
```

### Smoke-testing a fresh install

```bash
#!/bin/bash
set -euo pipefail

TOKEN=$(springtale-cli auth print)
HOST="http://127.0.0.1:8080"

echo "Health:"
curl -s "$HOST/health" | jq -e .status

echo "Connectors:"
curl -s "$HOST/connectors" -H "Authorization: Bearer $TOKEN" | jq '. | length'

echo "Rules:"
curl -s "$HOST/rules" -H "Authorization: Bearer $TOKEN" | jq '. | length'

echo "Formations:"
curl -s "$HOST/formations" -H "Authorization: Bearer $TOKEN" | jq '. | length'
```

## Error responses

All errors have a stable shape:

```json
{
  "error": {
    "code": "E007",
    "message": "Connector failed health check",
    "context": {
      "connector": "connector-telegram",
      "details": "tcp connect timeout"
    }
  }
}
```

HTTP status codes follow the usual conventions (200, 400, 401, 403,
404, 429, 500). The `code` field is the stable contract; status
codes may shift over time.

## Gotchas

- **Default bind is `127.0.0.1`.** Curl from a different machine
  needs the daemon's bind to allow remote, plus you crossed into
  threat-model territory — read [`docs/guide/security.md`](../../guide/security.md) §1 first.
- **SSE streams stay open.** `curl -N` is the right invocation; without
  `-N`, curl buffers and you see nothing until the connection
  closes.
- **The audit log includes every authenticated request.** Idle
  polling against the API generates audit rows. For heavy polling,
  prefer SSE.
- **JSON keys are snake_case** (Rust convention), not camelCase.
- **Timestamps are ISO-8601 with milliseconds**, always UTC.
