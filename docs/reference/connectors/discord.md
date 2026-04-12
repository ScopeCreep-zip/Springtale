# connector-discord

Discord integration via `twilight-gateway` and `twilight-http`. Slash commands, message ingestion, reactions, embeds. **Read the security note in §7 before deploying for sensitive use cases.**

```
  Discord gateway              connector-discord              Rule engine /
  (gateway.discord.gg)               │                         bot runtime
          │                          │                               │
          │ WebSocket (wss)          │                               │
          │  IDENTIFY + intents      │                               │
          │  ◄──────────────────────►│                               │
          │                          │                               │
          │ INTERACTION_CREATE       │                               │
          │ MESSAGE_CREATE (if       │                               │
          │   enable_message_content)│                               │
          │ MESSAGE_REACTION_ADD     │                               │
          ├─────────────────────────►│ emit interaction_received /   │
          │                          │      message_received / …     │
          │                          ├──────────────────────────────►│
          │                          │                               │
  Discord REST              │                               │
  (discord.com)             │                               │
          │ POST /channels/{id}/messages                            │
          │  Bot token                                              │
          │  ◄───────────────────────┤  execute(send_message,       │
          │                          │           send_embed, …)     │
          │                          │  ◄───────────────────────────┤
```

*Fig. 1. Discord data flow. Gateway WebSocket for ingest, REST for outbound. Message-content ingestion is gated on the `enable_message_content` privileged intent — enabling it means Discord transmits every watched-channel message to the bot.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|---|---|---|---|
| `bot_token` | `Secret<String>` | (required) | Bot token (format `NDc...`) |
| `application_id` | `u64` | (required) | Application ID for slash command registration |
| `guild_id` | `Option<u64>` | `None` | Guild ID for fast guild-scoped slash command install. If absent, commands register globally. |
| `enable_message_content` | `bool` | `false` | Request the `MESSAGE_CONTENT` privileged intent. **Warning:** enabling this means Discord transmits *all* message content in *all* watched channels to your bot. |
| `enable_direct_messages` | `bool` | `false` | Enable DM triggers |
| `enable_reactions` | `bool` | `false` | Enable reaction triggers |
| `message_jitter_secs` | `u64` | `0` | Random 0..N second delay on sends |
| `commands` | `Vec<SlashCommandConfig>` | `[]` | Slash commands to register (name, description) |

## 2. Authentication

Bot token authentication via twilight's HTTP client and gateway WebSocket. PAT-style token stored as `Secret<String>` and only exposed at the precise HTTP header call site.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | Description | Payload fields |
|---|---|---|
| `interaction_received` | Slash command or component interaction invoked | `interaction_id`, `command_name`, `user_id`, `channel_id`, `guild_id`, `options` |
| `message_received` | Message in a guild channel (requires `enable_message_content`) | `message_id`, `channel_id`, `guild_id`, `user_id`, `content`, `timestamp` |
| `dm_received` | Direct message (requires `enable_direct_messages`) | `message_id`, `channel_id`, `user_id`, `content`, `timestamp` |
| `reaction_added` | Reaction added (requires `enable_reactions`) | `user_id`, `channel_id`, `message_id`, `guild_id`, `emoji` |
| `member_joined` | User joins guild (requires `GUILD_MEMBERS` intent) | `user_id`, `guild_id`, `joined_at` |

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields |
|---|---|
| `send_message` | `channel_id`, `content` |
| `send_embed` | `channel_id`, `embed` (object) |
| `edit_message` | `channel_id`, `message_id`, `content` |
| `delete_message` | `channel_id`, `message_id` |
| `add_reaction` | `channel_id`, `message_id`, `emoji` |

## 5. Capabilities Required

| Capability | Parameter |
|---|---|
| `NetworkOutbound` | `discord.com` |
| `NetworkOutbound` | `gateway.discord.gg` |

## 6. Example Rule

```toml
[rule]
name = "discord-github-mirror"

[trigger]
type = "ConnectorEvent"
connector = "connector-github"
event = "push"

[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_message"

[actions.params]
channel_id = "123456789"
content = "New push to ${trigger.repository}: ${trigger.commits[0].message}"
```

## 7. Security & Privacy

**Discord is hostile infrastructure for vulnerable users.**

- Discord complies with government data requests including DHS subpoenas.
- Server admins can read every channel their bot has access to, and can add/remove bot access silently.
- Discord servers log connection IPs.
- Enabling `MESSAGE_CONTENT` means Discord sees and stores every message content string your bot observes — even if your rule never acts on it.

Use this connector for public-facing automation (game communities, support bots, dev tooling). Do **not** use it for asylum coordination, IPV safety planning, trans community mutual aid, or anything where Discord's cooperation with law enforcement is a threat vector. For those, use Signal, Nostr, or IRC over a VPN.
