# connector-kick

Kick streaming platform integration. OAuth 2.1 PKCE authentication, real-time webhook triggers for stream and chat events, and chat/channel/stream actions.

```
  Kick platform                 connector-kick                 Rule engine
       │                              │                             │
       │  OAuth 2.1 PKCE              │                             │
       │  (id.kick.com)               │                             │
       │  ◄──────────────────────────►│                             │
       │  access token                │                             │
       │                              │                             │
       │  webhook (stream_live,       │                             │
       │   chat_message, …)           │                             │
       │  + RSA signature             │                             │
       ├─────────────────────────────►│                             │
       │                              │ verify_webhook (RSA)        │
       │                              │ emit TriggerEvent           │
       │                              ├────────────────────────────►│
       │                              │                             │
       │  POST /chat/send, etc.       │  execute(send_chat, …)      │
       │  api.kick.com                │  ◄──────────────────────────┤
       │  ◄──────────────────────────┤                              │
```

*Fig. 1. Kick data flow. Inbound webhooks are RSA-verified against the Kick public key before being dispatched as trigger events; outbound actions hit the Kick REST API with a bearer token derived from the PKCE flow.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `client_id` | `String` | (required) | Kick OAuth application client ID |
| `client_secret` | `Secret<String>` | (required) | Kick OAuth application client secret |
| `redirect_uri` | `String` | (required) | OAuth redirect URI |
| `scopes` | `Vec<String>` | `["user:read", "channel:read", "channel:write", "chat:write", "events:subscribe"]` | OAuth scopes |
| `api_base` | `String` | `"https://api.kick.com"` | Kick API base URL |
| `oauth_base` | `String` | `"https://id.kick.com"` | Kick OAuth base URL |
| `webhook_callback_url` | `Option<String>` | `None` | Webhook callback URL for event subscriptions |

## 2. Authentication

OAuth 2.1 with PKCE (Proof Key for Code Exchange). The connector generates a code verifier/challenge pair, redirects the user to Kick's authorization endpoint, and exchanges the authorization code for an access token. No client secret is sent in the authorization request — PKCE prevents interception.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | Description | Payload fields |
|------|-------------|---------------|
| `chat_message` | A message was sent in chat | `message_id`, `content`, `sender` (User), `broadcaster` (User), `emotes`, `created_at` |
| `stream_live` | A stream went live | `broadcaster` (User), `is_live: true`, `title`, `started_at` |
| `stream_offline` | A stream went offline | `broadcaster` (User), `is_live: false`, `title`, `ended_at` |
| `channel_followed` | A channel was followed | `broadcaster` (User), `follower` (User) |

Triggers are delivered via Kick's webhook system. Events: `chat.message.sent`, `livestream.status.updated`, `channel.followed`.

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields | Output fields |
|------|-------------|--------------|
| `send_chat` | `channel_id: String`, `message: String` | `response: Object` |
| `get_channel` | `slug: String` | `channel: Object` |
| `get_stream` | `channel_id: String` | `stream: Object` |

## 5. Capabilities Required

| Capability | Parameter |
|-----------|-----------|
| `NetworkOutbound` | `api.kick.com` |
| `NetworkOutbound` | `id.kick.com` |

## 6. Example Rule

```toml
[rule]
name = "kick-stream-announce"

[trigger]
type = "ConnectorEvent"
connector = "connector-kick"
event = "stream_live"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.params]
text = "${trigger.broadcaster.username} is live on Kick: ${trigger.title}"
```

## 7. Webhook Verification

Inbound webhooks are verified using RSA signature verification. The `dispatch_raw_webhook` function checks the signature header against Kick's public key before processing the event payload.
