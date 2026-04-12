# connector-github

GitHub API v3 integration. Personal access token authentication, webhook-driven triggers with HMAC verification, and issue/comment/diff actions.

```
  GitHub                  connector-github               Rule engine
     │                            │                           │
     │  webhook (push, PR,        │                           │
     │   issue, comment)          │                           │
     │  X-Hub-Signature-256       │                           │
     ├───────────────────────────►│                           │
     │                            │ verify_webhook (HMAC-256) │
     │                            │ emit TriggerEvent         │
     │                            ├──────────────────────────►│
     │                            │                           │
     │  REST v3 request           │  execute(create_issue,    │
     │  api.github.com            │           post_comment,…) │
     │  Bearer <PAT>              │  ◄────────────────────────┤
     │  ◄─────────────────────────┤                           │
```

*Fig. 1. GitHub data flow. Inbound webhooks are HMAC-SHA256 verified against `webhook_secret`; outbound calls use a PAT stored as `Secret<String>`.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `token` | `Secret<String>` | (required) | GitHub Personal Access Token |
| `webhook_secret` | `Option<Secret<String>>` | `None` | HMAC-SHA256 webhook secret for signature verification |
| `api_base` | `String` | `"https://api.github.com"` | GitHub API base URL |

## 2. Authentication

Personal Access Token (PAT) passed as `Authorization: Bearer` header on all API requests.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | GitHub event | Payload fields |
|------|-------------|---------------|
| `push` | `push` | `ref`, `repository` (owner/repo), `pusher`, `commits_count` |
| `pull_request_opened` | `pull_request` (action: opened) | `number`, `title`, `repository` (owner/repo), `author`, `url` |
| `issue_opened` | `issues` (action: opened) | `number`, `title`, `repository` (owner/repo), `author`, `url` |
| `issue_comment` | `issue_comment` | `issue_number`, `body`, `repository` (owner/repo), `author` |

Payloads are transformed from GitHub's nested webhook JSON into flat fields by the daemon's webhook handler.

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields | Output fields |
|------|-------------|--------------|
| `create_issue` | `owner`, `repo`, `title`, `body` (optional, default: `""`) | `number: u64`, `url: String`, `response: Object` |
| `post_comment` | `owner`, `repo`, `issue_number: u64`, `body` | `id: u64`, `url: String`, `response: Object` |
| `get_diff` | `owner`, `repo`, `pull_number: u64` | `diff: String` |

## 5. Capabilities Required

| Capability | Parameter |
|-----------|-----------|
| `NetworkOutbound` | `api.github.com` |

## 6. Example Rule

```toml
[rule]
name = "pr-auto-comment"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "pull_request_opened"

[[conditions]]
type = "FieldEquals"
field = "trigger.repository"
value = "ScopeCreep-zip/Springtale"

[[actions]]
type = "RunConnector"
connector = "connector-github"
action = "post_comment"

[actions.params]
owner = "ScopeCreep-zip"
repo = "Springtale"
issue_number = "${trigger.number}"
body = "Thanks for the PR! CI checks will run automatically."
```

## 7. Webhook Verification

Inbound webhooks are verified using HMAC-SHA256. The `X-Hub-Signature-256` header contains `sha256=<hex-digest>`. The connector computes `HMAC-SHA256(webhook_secret, request_body)` and compares using constant-time equality. If `webhook_secret` is not configured, verification is skipped.
