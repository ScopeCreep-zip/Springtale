# API client: Python

Driving the `springtaled` HTTP API from Python. Uses `requests` for
sync and `httpx` for async examples.

For just modeling cooperation types (no daemon I/O), see
[`docs/python/`](../../python/). For full daemon control, this doc.

## Install

```bash
pip install requests httpx
# Optional but recommended:
pip install springtale         # the types crate from docs/python/
```

## A minimal client class

```python
from __future__ import annotations
import os
from typing import Any
import requests

class Springtale:
    def __init__(self, host: str = "http://127.0.0.1:8080", token: str | None = None):
        self.host = host.rstrip("/")
        self.token = token or self._read_token()
        self.session = requests.Session()
        self.session.headers["Authorization"] = f"Bearer {self.token}"
        self.session.headers["Content-Type"] = "application/json"

    def _read_token(self) -> str:
        path = os.path.expanduser("~/.local/share/springtale/api_token")
        with open(path) as f:
            return f.read().strip()

    def health(self) -> dict[str, Any]:
        r = self.session.get(f"{self.host}/health")
        r.raise_for_status()
        return r.json()

    def list_connectors(self) -> list[dict[str, Any]]:
        r = self.session.get(f"{self.host}/connectors")
        r.raise_for_status()
        return r.json()

    def list_rules(self) -> list[dict[str, Any]]:
        r = self.session.get(f"{self.host}/rules")
        r.raise_for_status()
        return r.json()

    def list_formations(self) -> list[dict[str, Any]]:
        r = self.session.get(f"{self.host}/formations")
        r.raise_for_status()
        return r.json()

    def deploy_formation(self, payload: dict[str, Any]) -> dict[str, Any]:
        r = self.session.post(f"{self.host}/formations/deploy-team", json=payload)
        r.raise_for_status()
        return r.json()

    def reload_connector(self, name: str) -> dict[str, Any]:
        r = self.session.post(f"{self.host}/connectors/{name}/reload")
        r.raise_for_status()
        return r.json()
```

Usage:

```python
sp = Springtale()
print(sp.health())                          # {'status': 'ok', ...}
print(len(sp.list_connectors()))            # 7
print([r["name"] for r in sp.list_rules()]) # ['welcome', 'flag-slurs', ...]
```

## Typed responses with `springtale-py`

If you've installed the bindings:

```python
import requests
from springtale import Formation

def fetch_formations(host: str, token: str) -> list[Formation]:
    r = requests.get(
        f"{host}/formations",
        headers={"Authorization": f"Bearer {token}"},
    )
    r.raise_for_status()
    return [Formation.from_dict(row) for row in r.json()]

for f in fetch_formations("http://127.0.0.1:8080", "<TOKEN>"):
    print(f.id, f.intent.kind, f.momentum.name)
```

Now your dashboard / monitoring / analysis code is type-checked end
to end.

## SSE streams (sync)

```python
import json
import requests

def stream_events(host: str, token: str):
    """Yield events one at a time as they arrive."""
    with requests.get(
        f"{host}/events/stream",
        params={"token": token},
        stream=True,
    ) as r:
        r.raise_for_status()
        event_type = None
        for line in r.iter_lines(decode_unicode=True):
            if not line:
                continue
            if line.startswith("event: "):
                event_type = line[len("event: "):]
            elif line.startswith("data: "):
                yield event_type, json.loads(line[len("data: "):])

for event_type, payload in stream_events("http://127.0.0.1:8080", "<TOKEN>"):
    print(event_type, payload)
```

## SSE streams (async with httpx)

```python
import asyncio
import json
import httpx

async def stream_events(host: str, token: str):
    async with httpx.AsyncClient(timeout=None) as client:
        async with client.stream(
            "GET",
            f"{host}/events/stream",
            params={"token": token},
        ) as r:
            r.raise_for_status()
            event_type = None
            async for line in r.aiter_lines():
                if not line:
                    continue
                if line.startswith("event: "):
                    event_type = line[len("event: "):]
                elif line.startswith("data: "):
                    yield event_type, json.loads(line[len("data: "):])

async def main():
    async for event_type, payload in stream_events(
        "http://127.0.0.1:8080", "<TOKEN>"
    ):
        print(event_type, payload)

asyncio.run(main())
```

## Watching cooperation events

```python
async for event_type, payload in stream_events_at(
    "http://127.0.0.1:8080/cooperation/events",
    "<TOKEN>",
    {"formation_id": "research-squad"},
):
    if event_type == "momentum_transition":
        print(f"{payload['formation_id']}: {payload['from']} -> {payload['to']}")
    elif event_type == "rally_burned":
        print(f"{payload['formation_id']}: rally burned ({payload['tokens_remaining']} left)")
```

## Error handling

```python
from requests.exceptions import HTTPError

try:
    sp.reload_connector("connector-nonexistent")
except HTTPError as e:
    if e.response.status_code == 404:
        print("connector not installed")
    elif e.response.status_code == 429:
        print("rate limited; retry later")
    else:
        body = e.response.json()
        print(f"E{body['error']['code']}: {body['error']['message']}")
        # Optionally:  springtale-cli fix E{code}  for a runbook
```

## Webhook signature verification

If you're receiving webhooks **from** Springtale (via an outbound
connector that delivers to your service), verify HMAC signatures:

```python
import hmac
import hashlib

def verify_webhook(payload_bytes: bytes, signature_header: str, secret: str) -> bool:
    """
    Verify the X-Springtale-Signature header on a webhook delivery.
    Signature is "sha256=<hex>".
    """
    expected = "sha256=" + hmac.new(
        secret.encode(),
        payload_bytes,
        hashlib.sha256,
    ).hexdigest()
    return hmac.compare_digest(expected, signature_header)

# In a Flask / FastAPI / Django handler:
if not verify_webhook(request.body, request.headers["X-Springtale-Signature"], MY_SECRET):
    return ("invalid signature", 401)
```

## Common workflows

### List active formations and their momentum

```python
formations = sp.list_formations()
for f in formations:
    if f.get("status") == "active":
        print(f"{f['id']}: {f['intent']['kind']} @ {f['momentum_tier']}")
```

### Trigger a rule manually

```python
sp.session.post(
    f"{sp.host}/rules/run",
    json={"rule_id": "stream-fanout"},
)
```

### Get audit trail for a connector

```python
audit = sp.session.get(
    f"{sp.host}/audit-trail",
    params={"connector": "connector-telegram", "limit": 100},
).json()
for row in audit:
    print(row["created_at"], row["verdict"], row["action_summary"])
```

## Gotchas

- **The token is sensitive.** Don't print it. Don't put it in URLs
  except the SSE query-string form. Don't commit it to a repo.
- **Local-only by default.** The daemon binds `127.0.0.1`. To call
  it from another machine, set up TLS + auth + maybe a tunnel — see
  [`docs/installation/docker.md`](../../installation/docker.md) on
  reverse proxies.
- **Schema may evolve pre-1.0.** Pin your client to a specific
  daemon version or test against the version you're targeting.
  After 1.0 ship, semver applies — see [`docs/operations/versioning-policy.md`](../../operations/versioning-policy.md).
- **No automatic retries.** A 502 from a transient daemon restart
  is your code's problem to handle.
- **Large responses.** `GET /events` without `limit` can return
  thousands of rows. Default `limit` server-side is 50; pagination
  via `?cursor=…` once you exceed.
