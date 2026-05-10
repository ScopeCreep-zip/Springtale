# connector-nostr

Nostr protocol integration. Pseudonymous, decentralised, relay-based. NIP-44 encrypted DMs (via NIP-17 gift wrap) and plaintext notes. Aligns closely with Springtale's threat model — best fit among chat connectors for vulnerable users.

```
  Nostr relays                    connector-nostr            Rule engine /
  (relay-1, relay-2, …)                 │                     bot runtime
          │                             │                            │
          │  WebSocket (wss)            │                            │
          │  ◄─────────────────────────►│  per-relay connection       │
          │  REQ (subscribe kinds 1,4,  │                            │
          │       gift-wrapped DMs)     │                            │
          │                             │                            │
          │  EVENT (kind 1 note)        │                            │
          ├────────────────────────────►│  emit note_received /      │
          │  EVENT (kind 1059 wrap)     │        mention_received    │
          ├────────────────────────────►│  decrypt NIP-44 gift wrap  │
          │                             │  emit dm_received          │
          │                             ├───────────────────────────►│
          │                             │                            │
          │  EVENT (signed)             │  execute(publish_note,     │
          │  + schnorr sig              │           send_dm, …)      │
          │  ◄──────────────────────────┤  ◄─────────────────────────┤

  Relay operators see your pubkey, the pubkeys you interact with
  (p-tags), the events you reply to (e-tags), and the timing.
  DMs are E2E encrypted; metadata is not.
```

*Fig. 1. Nostr data flow. Each configured relay holds its own WebSocket. DMs round-trip through the NIP-44 encryption layer before reaching the rule engine; message jitter (default 30 s) perturbs outgoing timing.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|---|---|---|---|
| `private_key` | `Secret<String>` | (required) | Nostr private key, `nsec` bech32 or hex. secp256k1, not Ed25519. |
| `relays` | `Vec<String>` | (required) | Relay URLs. At least one required. |
| `dm_encryption` | `String` | `"nip44"` | DM encryption scheme. `"nip44"` (modern) or `"nip04"` (legacy, deprecated). |
| `message_jitter_secs` | `u64` | `30` | Random 0..N second delay on sends, for social-graph obfuscation |

## 2. Authentication

Ed25519-style identity via secp256k1 Schnorr signatures (BIP-340). Note: this is **separate from Springtale's main Ed25519 node identity** — Nostr requires its own keypair because the protocol is specified against secp256k1.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | Description | Payload fields |
|---|---|---|
| `note_received` | Text note (kind 1) from subscribed relays | `event_id`, `pubkey`, `content`, `created_at`, `relay_url` |
| `dm_received` | NIP-17 / NIP-44 gift-wrapped DM, decrypted | `sender_pubkey`, `content` (decrypted), `created_at`, `relay_url` |
| `mention_received` | Bot's pubkey mentioned via p-tag | `event_id`, `pubkey`, `content`, `created_at` |
| `reaction_received` | Reaction (kind 7) to one of the bot's events | `event_id`, `pubkey`, `content` (emoji), `target_event_id` |

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields |
|---|---|
| `publish_note` | `content` |
| `send_dm` | `recipient_pubkey`, `content` |
| `send_message` | `text` (required), `chat_id` (optional) — unified entrypoint that routes to `send_dm` when `chat_id` looks like a pubkey (hex or `npub1…`), or to `publish_note` when omitted. Used by the bot router. |
| `react` | `event_id`, `emoji` |
| `reply` | `event_id`, `content` |

## 5. Capabilities Required

| Capability | Parameter |
|---|---|
| `NetworkOutbound` | one entry per configured relay (e.g. `relay.damus.io`) |

## 6. Example Rule

```toml
[rule]
name = "nostr-daily-digest"

[trigger]
type = "Cron"
expression = "0 9 * * *"

[[actions]]
type = "RunConnector"
connector = "connector-nostr"
action = "publish_note"

[actions.params]
content = "Daily digest: ${trigger.summary}"
```

## 7. Security & Privacy

- **Pseudonymous by design.** No phone number, no email, no real name. Identity is a secp256k1 keypair.
- **E2E encrypted DMs via NIP-44.** Messages are unreadable by relay operators.
- **Metadata leakage.** Relay operators still see: your pubkey, the pubkeys you interact with (via p-tags), the event IDs you reply to (via e-tags), and the timing of your posts. Use multiple relays and the message jitter to blunt social-graph correlation.
- **No central server.** Relays are untrusted — publish to multiple, read from multiple, and rotate as needed.
- NIP-04 DM encryption is deprecated due to metadata leakage and is kept only for legacy interoperability. Default to NIP-44.
