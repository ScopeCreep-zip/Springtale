# connector-slack

Slack integration via Socket Mode (WebSocket) and the Web API. Slash commands, messages, reactions, thread replies, Block Kit layouts. **Hostile infrastructure for vulnerable users — see §7.**

```
  Slack Socket Mode             connector-slack             Rule engine /
  (wss-primary.slack.com)             │                      bot runtime
          │                           │                            │
          │  WebSocket                │                            │
          │  xapp-… app token         │                            │
          │  ◄───────────────────────►│                            │
          │                           │                            │
          │  slash_commands / events  │                            │
          ├──────────────────────────►│ emit slash_command /       │
          │                           │      message_received /    │
          │                           │      app_mentioned / …     │
          │                           ├───────────────────────────►│
          │                           │                            │
  Slack Web API            │                            │
  (slack.com)              │                            │
          │  POST chat.postMessage                                  │
          │  xoxb-… bot token                                       │
          │  ◄────────────────────────┤  execute(send_message,      │
          │                           │           send_blocks, …)  │
          │                           │  ◄──────────────────────────┤

  Workspace admins see EVERY channel and DM this bot has access to,
  with no user-visible notification. See §7 for the full threat note.
```

*Fig. 1. Slack data flow. Socket Mode for ingest (via `app_token`), Web API for outbound (via `bot_token`). Both tokens are `Secret<String>`. Workspace admins can silently read everything the bot observes — this is a property of Slack, not of the connector.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|---|---|---|---|
| `bot_token` | `Secret<String>` | (required) | Bot user OAuth token (`xoxb-...`) |
| `app_token` | `Secret<String>` | (required) | App-level token (`xapp-...`) for Socket Mode |
| `message_jitter_secs` | `u64` | `0` | Random 0..N second delay on sends |

## 2. Authentication

Two tokens. `bot_token` (`xoxb-`) authenticates Web API calls via `Authorization: Bearer`. `app_token` (`xapp-`) authenticates the Socket Mode WebSocket. Both wrapped in `Secret<String>`.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | Description | Payload fields |
|---|---|---|
| `slash_command` | Slash command invoked via Socket Mode | `command`, `text`, `user_id`, `channel_id`, `response_url` |
| `message_received` | Message in a channel the bot is in | `user_id`, `channel_id`, `text`, `ts` |
| `app_mentioned` | Bot `@mentioned` in a channel | `user_id`, `channel_id`, `text`, `ts` |
| `reaction_added` | Emoji reaction added to a message | `user_id`, `reaction`, `item_channel`, `item_ts` |
| `thread_reply` | Reply in a thread | `user_id`, `channel_id`, `text`, `ts`, `thread_ts` |

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields |
|---|---|
| `send_message` | `channel_id`, `text` |
| `send_blocks` | `channel_id`, `blocks` (Block Kit array) |
| `send_thread_reply` | `channel_id`, `thread_ts`, `text` |
| `edit_message` | `channel_id`, `ts`, `text` |
| `add_reaction` | `channel_id`, `ts`, `emoji` |

## 5. Capabilities Required

| Capability | Parameter |
|---|---|
| `NetworkOutbound` | `slack.com` |
| `NetworkOutbound` | `wss-primary.slack.com` |

## 6. Example Rule

```toml
[rule]
name = "slack-deploy-announce"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "push"

[[conditions]]
type = "FieldEquals"
field = "trigger.ref"
value = "refs/heads/main"

[[actions]]
type = "RunConnector"
connector = "connector-slack"
action = "send_message"

[actions.params]
channel_id = "C012AB3CD"
text = "Deploying ${trigger.commits[0].message} to production"
```

## 7. Security & Privacy

**Slack is hostile infrastructure for vulnerable users. Think twice before using it for anything sensitive.**

- **Workspace admins can read every message — including DMs — with no notification.** This has been the case since Slack's 2018 policy change. Users cannot opt out.
- **Enterprise Grid adds compliance export and eDiscovery.** Everything is recoverable by the workspace owner, forever.
- **Slack complies with government data requests.** Assume subpoenaed data will be produced.
- **Tokens are revocable silently** by workspace admins. A bot can lose access without warning.
- **Data retention is admin-controlled.** Users cannot enforce their own retention on their own messages.

Use this connector for public-facing workplace automation (CI alerts, deploy announcements, on-call rotation). Do **not** use it for asylum coordination, IPV safety planning, trans community mutual aid, labor organising in a hostile workplace, or anything where workspace-admin or law-enforcement access is a threat vector. For those, use Signal, Nostr, or IRC over a VPN.
