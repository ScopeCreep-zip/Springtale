# connector-http

Generic HTTP client for making GET and POST requests to allow-listed hosts. Useful for integrating with APIs that don't have a dedicated connector.

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `allowed_hosts` | `Vec<String>` | (required) | Exact hostnames allowed for requests (no wildcards) |
| `default_headers` | `HashMap<String, String>` | `{}` | Headers added to every request |
| `timeout_secs` | `u64` | `30` | Request timeout in seconds |

## 2. Authentication

None built-in. Pass bearer tokens or API keys via `default_headers` or per-request `headers`.

## 3. Triggers

None. This is an action-only connector.

## 4. Actions

**TABLE II. ACTIONS**

| Name | Input fields | Output fields |
|------|-------------|--------------|
| `get` | `url: String`, `headers: Map` (default: `{}`) | `status: u16`, `headers: Map`, `body: String` |
| `post` | `url: String`, `headers: Map` (default: `{}`), `body: String` (default: `""`) | `status: u16`, `headers: Map`, `body: String` |

## 5. Capabilities Required

Capabilities are generated dynamically from configuration:

| Capability | Parameter |
|-----------|-----------|
| `NetworkOutbound` | Each host in `allowed_hosts` |

## 6. Example Rule

```toml
[rule]
name = "webhook-forward"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "push"

[[actions]]
type = "RunConnector"
connector = "connector-http"
action = "post"

[actions.params]
url = "https://notify.example.com/deploy"
headers = { "Content-Type" = "application/json" }
body = '{"repo": "${trigger.repository}", "ref": "${trigger.ref}"}'
```

## 7. Security Notes

- **Host validation**: Every request URL is checked against `allowed_hosts` before execution. Exact host match — no wildcards, no subdomains.
- **rustls-tls**: All HTTPS requests use rustls. native-tls is banned at compile time.
- **No redirects to disallowed hosts**: If a server redirects to a host not in `allowed_hosts`, the request fails.
