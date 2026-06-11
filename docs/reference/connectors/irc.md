# connector-irc

Native IRC client. Channel messages, commands, topic changes, joins, parts. TLS-by-default with optional SASL PLAIN for networks that require it.

```
  IRC server                     connector-irc               Rule engine /
                                       │                      bot runtime
          │                            │                            │
          │  TLS handshake (rustls)    │                            │
          │  ◄────────────────────────►│                            │
          │  CAP REQ sasl (optional)   │                            │
          │  AUTHENTICATE PLAIN        │                            │
          │  NICK, USER, JOIN          │                            │
          │  ◄────────────────────────►│                            │
          │                            │                            │
          │  PRIVMSG #chan :text       │                            │
          ├───────────────────────────►│ parse, split command_prefix│
          │                            │ emit message_received /    │
          │                            │      command_received /    │
          │                            │      user_joined / …       │
          │                            ├───────────────────────────►│
          │                            │                            │
          │  PRIVMSG #chan :reply      │  execute(send_message,     │
          │  (+ message_jitter_secs)   │           join_channel, …) │
          │  ◄─────────────────────────┤  ◄─────────────────────────┤
```

*Fig. 1. IRC data flow. TLS protects the client→server link; server operators and other users on the server still see every message. The default 15-second message jitter blunts timing correlation on send.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|---|---|---|---|
| `server` | `String` | (required) | IRC server hostname (e.g. `irc.libera.chat`) |
| `port` | `u16` | `6697` | Server port |
| `use_tls` | `bool` | `true` | Use TLS. Must be `true` for production. |
| `nick` | `String` | (required) | Bot nickname |
| `nickserv_password` | `Option<Secret<String>>` | `None` | NickServ password |
| `sasl_enabled` | `bool` | `false` | Enable SASL PLAIN (required on Libera.Chat and similar) |
| `channels` | `Vec<String>` | `[]` | Channels to auto-join |
| `command_prefix` | `String` | `"!"` | Command prefix |
| `message_jitter_secs` | `u64` | `15` | Random 0..N second delay on sends, for social-graph obfuscation |

## 2. Authentication

SASL PLAIN (modern) or NickServ password (legacy). Credentials are wrapped in `Secret<String>` and exposed only at the authentication handshake. No end-to-end encryption — TLS protects the connection to the server, not the message contents from server operators.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | Description | Payload fields |
|---|---|---|
| `message_received` | Message in a joined channel or a DM | `nick`, `target`, `message`, `host` |
| `command_received` | Message starting with `command_prefix` | `nick`, `target`, `command`, `args`, `message` |
| `user_joined` | User joins a channel | `nick`, `channel` |
| `user_parted` | User leaves a channel | `nick`, `channel`, `reason` |
| `topic_changed` | Channel topic changes | `channel`, `topic`, `nick` |

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields |
|---|---|
| `send_message` | `target`, `text` |
| `join_channel` | `channel` |
| `part_channel` | `channel`, optional `reason` |
| `set_topic` | `channel`, `topic` |
| `send_action` | `target`, `text` (CTCP ACTION, i.e. `/me`) |
| `discover_destinations` | — (read-only; returns a `workspaces` array of `{workspace_key, display_name, kind, metadata}` — feeds the D1 external-workspaces directory, the 🔍 Scan affordance) |

## 5. Capabilities Required

| Capability | Parameter |
|---|---|
| `NetworkOutbound` | configured `server` hostname |

## 6. Example Rule

```toml
[rule]
name = "irc-help-responder"

[trigger]
type = "ConnectorEvent"
connector = "connector-irc"
event = "command_received"

[[conditions]]
type = "FieldEquals"
field = "trigger.command"
value = "help"

[[actions]]
type = "RunConnector"
connector = "connector-irc"
action = "send_message"

[actions.params]
target = "${trigger.target}"
text = "${trigger.nick}: see https://example.com/docs"
```

## 7. Security & Privacy

- **No end-to-end encryption.** Server operators and network observers on the server side can read all message content. TLS protects the client → server link only.
- **IP leakage via WHOIS.** Your connecting IP is visible on most networks unless you use a bouncer (ZNC) or a VPN + cloaked hostmask.
- **Message jitter defaults to 15 seconds** to make social-graph correlation harder. Override if timing matters more than privacy.
- Not recommended for covert organising. Use Signal, Nostr, or Matrix (when the CVE situation is resolved) instead.
