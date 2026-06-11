# connector-telegram

Telegram Bot API integration. Polling or webhook message ingestion, commands, inline keyboards, photos, edits.

```
  Telegram servers            connector-telegram              Rule engine /
  (api.telegram.org)                 │                          bot runtime
          │                          │                               │
          │ getUpdates (long poll)   │                               │
          │  ◄──────────────────────►│                               │
          │   OR                     │                               │
          │ POST webhook             │                               │
          ├─────────────────────────►│                               │
          │                          │ parse Update                  │
          │                          │ emit message_received or      │
          │                          │      command_received         │
          │                          ├──────────────────────────────►│
          │                          │                               │
          │ sendMessage / sendPhoto  │  execute(send_message, …)     │
          │  ◄───────────────────────┤  ◄────────────────────────────┤
```

*Fig. 1. Telegram data flow. Selectable between long-polling (`update_mode = "polling"`) and webhook (`update_mode = "webhook"`). All Bot API calls go to `api.telegram.org` with the bot token; Telegram servers see all content — there is no end-to-end encryption on the Bot API.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|---|---|---|---|
| `bot_token` | `Secret<String>` | (required) | Bot token from `@BotFather` (format `123456:ABC-DEF...`) |
| `api_base` | `String` | `"https://api.telegram.org"` | Telegram Bot API base URL |
| `update_mode` | `String` | `"polling"` | `"polling"` or `"webhook"` |
| `webhook_url` | `Option<String>` | `None` | Callback URL, required when `update_mode = "webhook"` |
| `webhook_secret` | `Option<Secret<String>>` | `None` | Webhook secret token, required when `update_mode = "webhook"`. Telegram echoes this on every callback in the `X-Telegram-Bot-Api-Secret-Token` header; the daemon rejects requests where it doesn't match. |
| `poll_timeout` | `u64` | `30` | Long-polling timeout in seconds |

## 2. Authentication

Bot token passed as a URL path segment in Bot API requests. The token is wrapped in `Secret<String>` and exposed only at the exact request-construction site. No OAuth, no PKCE — Telegram's Bot API is a shared-secret model.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | Description | Payload fields |
|---|---|---|
| `message_received` | Any message in a chat the bot can see | `message_id`, `chat` (object), `from` (object), `text`, `date` |
| `command_received` | Bot command (`/command`) received | `message_id`, `chat`, `from`, `command`, `args`, `text`, `date` |
| `callback_query_received` | A user tapped an inline keyboard button (Telegram callback query) | `callback_query_id`, `from`, `message`, `data` |

Handlers for `callback_query_received` should call `answer_callback_query` within 10 seconds, per the Bot API spec — otherwise Telegram's UI spins indefinitely.

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields |
|---|---|
| `send_message` | `chat_id`, `text` |
| `send_photo` | `chat_id`, `photo` (URL or `file_id`), `caption` |
| `edit_message` | `chat_id`, `message_id`, `text` |
| `delete_message` | `chat_id`, `message_id` |
| `send_inline_keyboard` | `chat_id`, `text`, `keyboard` (array of button arrays) |
| `answer_callback_query` | `callback_query_id`, `text` (toast), `show_alert` (bool) |
| `discover_destinations` | — (read-only; returns a `workspaces` array of `{workspace_key, display_name, kind, metadata}` — feeds the D1 external-workspaces directory, the 🔍 Scan affordance) |

## 5. Capabilities Required

| Capability | Parameter |
|---|---|
| `NetworkOutbound` | `api.telegram.org` |

## 6. Example Rule

```toml
[rule]
name = "telegram-help-responder"

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "command_received"

[[conditions]]
type = "FieldEquals"
field = "trigger.command"
value = "help"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${trigger.chat.id}"
text = "Available commands: /search, /status, /help"
```

## 7. Security & Privacy

- **No end-to-end encryption between the bot and Telegram.** Bot API requests transit plaintext over HTTPS to `api.telegram.org`. Telegram servers see and store all message content the bot observes.
- Telegram "Secret Chats" (the E2E feature) are a user-to-user feature and do not exist in the Bot API.
- Reasonable for public bots, community automations, and notification workflows. Not appropriate for sensitive organising — Telegram complies with government requests in many jurisdictions and has been subpoenaed successfully.
- Webhook mode: your webhook URL needs to be reachable by Telegram's servers. Behind `tower-http` rate limiting, with bearer auth enforced by the connector's own signature check.
