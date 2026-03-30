# connector-presearch

Presearch decentralized search engine integration. Search and scrape actions with TTL-based result caching.

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `api_key` | `Secret<String>` | (required) | Presearch API key |
| `api_base` | `String` | `"https://presearch.com"` | Presearch API base URL |
| `cache_ttl_secs` | `u64` | `300` (5 minutes) | Cache TTL for search/scrape results |
| `allowed_scrape_hosts` | `Vec<String>` | `[]` | Additional hosts allowed for scrape action |

## 2. Authentication

API key passed as a header on each request.

## 3. Triggers

None. This is an action-only connector.

## 4. Actions

**TABLE II. ACTIONS**

| Name | Input fields | Output fields |
|------|-------------|--------------|
| `search` | `query: String` | `results: Object`, `cached: bool` |
| `scrape` | `url: String` | `content: String`, `cached: bool` |

Both actions use the same TTL-based cache. Repeated identical requests within the TTL window return cached results (indicated by `cached: true`).

## 5. Capabilities Required

| Capability | Parameter |
|-----------|-----------|
| `NetworkOutbound` | `presearch.com` |
| `NetworkOutbound` | Each host in `allowed_scrape_hosts` |

## 6. Example Rule

```toml
[rule]
name = "scheduled-search"

[trigger]
type = "Cron"
expression = "0 9 * * *"

[[actions]]
type = "RunConnector"
connector = "connector-presearch"
action = "search"

[actions.params]
query = "Springtale automation platform"
```
