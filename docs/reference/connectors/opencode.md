# connector-opencode

Agentic coding via a locally-running [`opencode serve`](https://opencode.ai) daemon. A bot hands off coding tasks in plain language — "fix this bug", "add tests", "refactor X" — and gets the agent's reply back. The daemon does the file edits and command runs on the host; from Springtale's sandbox the connector only makes HTTP calls to the loopback daemon.

```
  Bot / rule engine        connector-opencode          opencode serve
        │                          │                  (127.0.0.1:4096)
        │ execute(run_task,        │                          │
        │   {prompt})              │                          │
        ├─────────────────────────►│ POST /session            │
        │                          ├─────────────────────────►│
        │                          │ POST /session/{id}/message
        │                          ├─────────────────────────►│
        │                          │      (agent edits files, │
        │                          │       runs commands)     │
        │                          │◄─────────────────────────┤
        │ {session_id, reply}      │  info + parts            │
        │◄─────────────────────────┤                          │
```

*Fig. 1. OpenCode data flow. Action-only — no triggers. Coding actions are `read_only: false`, so the chat-approval gate (W2) fronts them.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `base_url` | `String` | `"http://127.0.0.1:4096"` | Base URL of the running `opencode serve` daemon |
| `password` | `Option<Secret<String>>` | `None` | Daemon HTTP basic-auth password (its `OPENCODE_SERVER_PASSWORD`). Omit when the daemon runs without auth |
| `model` | `Option<String>` | `None` | Model id passed through on each prompt (e.g. `anthropic/claude-sonnet-4`). `None` uses the daemon's configured default |
| `agent` | `Option<String>` | `None` | Route prompts to a specific opencode agent |

## 2. Authentication

HTTP basic auth against the daemon. The username is fixed (`opencode`); only the password varies. When `password` is unset, requests are sent unauthenticated — fine for a loopback-only daemon.

## 3. Triggers

None. The connector is action-only; all work flows to the local daemon on demand.

## 4. Actions

**TABLE II. ACTIONS**

| Name | Input fields | Output fields |
|------|-------------|--------------|
| `run_task` | `prompt` (the coding task, in plain language), `title` (optional session title for later reference) | `session_id`, `reply` (the agent's text reply), `response` (raw message response: info + parts) |
| `continue_session` | `session_id` (from a previous `run_task`), `prompt` (the follow-up instruction, e.g. "now add tests") | `session_id`, `reply`, `response` |

Both actions are `read_only: false` — agentic coding edits files and runs commands on the host, so they are never auto-granted under a `Reconnoiter` intent and the approval gate fronts them.

## 5. Capabilities Required

| Capability | Parameter |
|-----------|-----------|
| `NetworkOutbound` | host:port parsed from `base_url` (default `127.0.0.1:4096`) |

## 6. Example Rule

```toml
[rule]
name = "fix-on-demand"

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "message_received"

[[conditions]]
type = "FieldContains"
field = "trigger.text"
value = "/fix"

[[actions]]
type = "RunConnector"
connector = "connector-opencode"
action = "run_task"

[actions.params]
prompt = "${trigger.text}"
title = "telegram-requested fix"
```

## 7. Security Notes

- **The daemon is the trust boundary.** `opencode serve` edits files and executes commands with the privileges of whoever started it. Springtale's sandbox constrains the connector (loopback HTTP only), not the daemon. Run the daemon in the workspace you intend it to touch, nothing wider.
- **Approval-gated.** Because both actions are mutating, formation intent decomposition never selects them for monitor-only intents, and interactive surfaces require explicit approval before dispatch.
- **Keep it loopback.** Pointing `base_url` at a non-loopback host sends your prompts (and the agent's file contents) over the network. Don't, unless you control the transport end-to-end.
