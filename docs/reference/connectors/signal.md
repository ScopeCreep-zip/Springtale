# connector-signal

Signal Protocol messaging via a `signal-cli` bridge. End-to-end encrypted messaging, group chats, and disappearing message timers. All Signal Protocol crypto is handled by `signal-cli` out-of-process.

```
  Signal servers          signal-cli            connector-signal         Rule engine /
                          daemon                       │                 bot runtime
         │                   │                         │                        │
         │  Signal           │                         │                        │
         │  Protocol (E2E)   │                         │                        │
         │  ◄───────────────►│                         │                        │
         │                   │ JSON-RPC over HTTP      │                        │
         │                   │  ◄─────────────────────►│                        │
         │                   │                         │                        │
         │  incoming msg     │                         │                        │
         │  ◄────────────────┤ event push              │                        │
         │                   ├────────────────────────►│ emit message_received /│
         │                   │                         │      group_message_…   │
         │                   │                         ├───────────────────────►│
         │                   │                         │                        │
         │                   │ send                    │ execute(send_message, …)│
         │                   │  ◄──────────────────────┤  ◄─────────────────────┤
         │                   │                         │                        │

  Phone number + keys live inside signal-cli only — Springtale never
  touches them. signal-cli stores its message DB in plaintext on local
  disk; see §7 for the device-seizure implication.
```

*Fig. 1. Signal data flow. Springtale never holds Signal Protocol keys or the account phone number — it brokers JSON-RPC calls to a local `signal-cli` daemon which handles all E2E crypto.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|---|---|---|---|
| `daemon_url` | `String` | (required) | `signal-cli` daemon HTTP endpoint, e.g. `http://localhost:8080` |
| `account_id` | `String` | (required) | Account identifier — UUID or local alias. **Not the phone number**, which is stored only by `signal-cli`. |
| `message_jitter_secs` | `u64` | `0` | Random 0..N second delay on sends |

## 2. Authentication

HTTP JSON-RPC to the local `signal-cli` daemon. The daemon holds Signal Protocol credentials; Springtale never touches them. The phone number associated with the Signal account lives in `signal-cli`'s data directory, never in Springtale config.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | Description | Payload fields |
|---|---|---|
| `message_received` | 1:1 Signal message | `source`, `message`, `timestamp` (unix ms), `expires_in_seconds` |
| `group_message_received` | Group message | `source`, `group_id`, `message`, `timestamp` |
| `disappearing_timer_changed` | Disappearing-message timer was changed | `source`, `expires_in_seconds` |

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields | Output |
|---|---|---|
| `send_message` | `text`, `recipients` (array) or `chat_id` (single) | `timestamp` |
| `send_group_message` | `group_id`, `text` | `timestamp` |
| `set_disappearing_timer` | `source`, `expires_in_seconds` (`0` = disabled) | — |
| `discover_destinations` | — (read-only) | `workspaces` array of `{workspace_key, display_name, kind, metadata}` — feeds the D1 external-workspaces directory (the 🔍 Scan affordance) |

## 5. Capabilities Required

| Capability | Parameter |
|---|---|
| `NetworkOutbound` | `localhost` (or whatever host serves `signal-cli`) |

## 6. Example Rule

```toml
[rule]
name = "signal-backup-notify"

[trigger]
type = "Cron"
expression = "0 2 * * *"

[[actions]]
type = "RunConnector"
connector = "connector-signal"
action = "send_message"

[actions.params]
chat_id = "+15551234567"
text = "Nightly backup complete."
```

If you want a shell command to run before the Signal notification (e.g. to run a backup), put a separate `RunShell` action *first* — see [`shell.md`](shell.md) for the action contract and the allow-list semantics.

## 7. Security & Privacy

- **E2E encrypted in transit.** Signal's servers cannot read message contents.
- **`signal-cli` stores the message database and keys in plaintext on local disk** (`~/.local/share/signal-cli/data/`). For device seizure protection, you need OS-level full-disk encryption — Springtale's vault does not cover `signal-cli`'s storage. Running Springtale in `ephemeral = true` mode only protects Springtale's own state.
- **Use a VPN** if you don't want Signal to know your IP.
- **Phone number visibility.** Signal accounts are phone-number based. The number is stored in `signal-cli`, not Springtale, but it is still discoverable by Signal itself and by anyone who has it in their contacts.
