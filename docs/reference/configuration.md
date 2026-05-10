# Configuration Reference

Springtale is configured via a TOML file and optional environment variable overrides.

## 1. File Location

The daemon looks for `springtale.toml` in the current working directory. Override with the `SPRINGTALE_CONFIG` environment variable or `--config` flag.

A minimal config is **empty** — every section has safe defaults. You only write the keys you need to override.

```
  springtale.toml
  │
  ├── ephemeral, heartbeat_interval_secs        ─── §2 top-level
  │
  ├── [store]              path, retention_days         ─── §3
  ├── [crypto]             vault_path                   ─── §4
  ├── [transport]          transport_type, socket_path  ─── §5
  │     └── [transport.http]  listen_addr, tls_*         ─── §5.1
  ├── [api]                bind, rate_limit_per_sec     ─── §6
  │
  ├── [ai_ollama]                                       ─── §7.1 ┐
  ├── [ai_openai]                                       ─── §7.2 ├─ optional
  ├── [ai_anthropic]                                    ─── §7.3 ┘  AI adapters
  │
  ├── [bot]                context_window, vault_timeout_secs  ─── §8
  │     └── [bot.persona]  name, tone, prefix          ─── §8.1
  │
  ├── [sentinel]           rate limits, breaker, dead-man ─── §9
  ├── [cooperation]        cross-process gossip (chitchat+SWIM) ─── §9.1
  │
  ├── [telegram]  [discord]  [slack]  [irc]  [nostr]  [signal]    ─── §10
  │    chat connectors — absent section = connector not loaded
  │
  └── [kick]  [github]  [bluesky]  [presearch]                     ─── §11
      [http]  [filesystem]  [shell]  [browser]
      service connectors
```

*Fig. 1. Configuration hierarchy. Everything after the top-level keys is optional; omitting a section means that subsystem runs with defaults (or, for optional connectors, doesn't run at all).*

---

## 2. Top-Level Keys

**TABLE I. ROOT CONFIG KEYS**

| Key | Type | Default | Description |
|---|---|---|---|
| `ephemeral` | `bool` | `false` | In-memory mode. All state lost on exit. Used for travel mode, demos, privacy-critical terminals. **No persistence.** |
| `heartbeat_interval_secs` | `u64` | `1800` (30 min) | Heartbeat tick interval. `0` to disable. |

---

## 3. `[store]`

| Key | Type | Default | Description |
|---|---|---|---|
| `path` | `PathBuf` | `~/.local/share/springtale/springtale.db` | SQLite database file path. Validated as a safe path. |
| `ephemeral` | `bool` | `false` | In-memory backend. Lost on exit. Equivalent to the top-level `ephemeral` for just the store. |
| `encryption_key_hex` | `Option<String>` | `None` | Hex-encoded 32-byte key for SQLite encryption at rest via SQLite3MultipleCiphers (ChaCha20-Poly1305). Normally derived from the vault passphrase during boot — setting this manually bypasses derivation. |
| `retention_days` | `Option<u32>` | `None` | Purge events and audit logs older than N days. Hourly background task; `None` keeps forever. |

---

## 4. `[crypto]`

| Key | Type | Default | Description |
|---|---|---|---|
| `vault_path` | `PathBuf` | `~/.local/share/springtale/vault.bin` | Encrypted vault file path. |

**Note on duress:** the duress passphrase and decoy vault are configured via `springtale vault duress-setup` at runtime, not via the config file. Both real and duress passphrases operate on the same `vault_path` — the file contains two encrypted regions with a constant total size of 131,152 bytes.

---

## 5. `[transport]`

| Key | Type | Default | Description |
|---|---|---|---|
| `transport_type` | `String` | `"local"` | `"local"` (Unix socket) or `"http"` (rustls mTLS) |
| `socket_path` | `PathBuf` | `~/.local/share/springtale/springtale.sock` | Unix domain socket path (when `transport_type = "local"`) |
| `[transport.http]` | table | absent | Required when `transport_type = "http"`. Sub-table with `bind`, `server_cert`, `server_key`, `client_ca` (PEM paths). |

---

## 6. `[api]`

| Key | Type | Default | Description |
|---|---|---|---|
| `bind` | `String` | `"127.0.0.1:8080"` | Management API listen address. **Warning:** binding `0.0.0.0` exposes the API to the network — only do this behind a reverse proxy. |
| `rate_limit_per_sec` | `u32` | `100` | Rate limit per second. Range 1–10000. |

---

## 7. AI Adapters

All three are optional. If absent, `NoopAdapter` is used (the platform works fully without AI). Multiple may be configured — the active one is selected at runtime via `POST /config/ai` and can be hot-swapped.

### 7.1 `[ai_ollama]`

Local models via Ollama. Nothing leaves the device.

| Key | Type | Default | Description |
|---|---|---|---|
| `base_url` | `String` | `"http://127.0.0.1:11434"` | Ollama HTTP endpoint |
| `model` | `String` | `"llama3.2"` | Model name, e.g. `"llama3.2"`, `"llama3.1:8b"` |

### 7.2 `[ai_openai]`

Any OpenAI-compatible endpoint (OpenAI, Gemini, DeepSeek, llama.cpp, vLLM).

| Key | Type | Default | Description |
|---|---|---|---|
| `base_url` | `String` | (required) | Base URL ending before `/chat/completions` |
| `api_key` | `Secret<String>` | (required) | API key — stored encrypted in the vault, never serialized |
| `model` | `String` | (required) | Model name |

SSE streaming is fully supported. Tool calling routes through `complete_with_tools()` (non-streaming) since argument JSON must be complete before tool execution — the streaming path returns text deltas and final `finish_reason`.

### 7.3 `[ai_anthropic]`

Anthropic Claude API.

| Key | Type | Default | Description |
|---|---|---|---|
| `api_key` | `Secret<String>` | (required) | API key |
| `model` | `String` | `"claude-sonnet-4-20250514"` | Model name |
| `base_url` | `String` | `"https://api.anthropic.com"` | API base URL |

Full SSE streaming.

---

## 8. `[bot]`

Bot runtime configuration. If absent, the bot is disabled — rules and connectors still work, the bot event loop simply doesn't start.

| Key | Type | Default | Description |
|---|---|---|---|
| `context_window` | `usize` | `50` | Conversation context window size (entries kept in memory for AI fallback) |
| `vault_timeout_secs` | `u64` | `300` | Auto-lock timeout in seconds |
| `[bot.persona]` | table | (defaults) | Persona block |

### 8.1 `[bot.persona]`

| Key | Type | Default | Description |
|---|---|---|---|
| `name` | `String` | `"Springtale"` | Bot display name |
| `tone` | `String` | `"neutral"` | Response tone hint used by the AI fallback adapter |
| `prefix` | `char` | `'/'` | Command prefix character |

See `docs/intended-arch/COOPERATION.md` for the cadence / formation / momentum model.

---

## 9. `[sentinel]`

Behavioural monitor configuration. If absent, the sentinel runs with the defaults below. There is no "disable sentinel" mode.

| Key | Type | Default | Description |
|---|---|---|---|
| `rate_limit_per_minute` | `u32` | `60` | Max actions per minute per connector |
| `circuit_breaker_threshold` | `u32` | `3` | Consecutive failures before the breaker opens |
| `circuit_breaker_cooldown_secs` | `u64` | `300` | Cooldown before an opened breaker re-arms |
| `dead_man_threshold` | `u32` | `120` | Actions per minute before the dead-man switch trips |
| `audit_retention_days` | `u32` | `90` | Audit trail retention |

The sentinel also checks toxic capability pairs at manifest install time and writes a row to the `audit_trail` table for every dispatch decision.

---

## 9.1. `[cooperation]`

Cooperation-layer runtime config. Controls how formations gossip when
multiple `springtaled` processes run on the same machine or LAN. Default
is single-process with an in-memory gossip store — nothing to configure.

| Key | Type | Default | Description |
|---|---|---|---|
| `cross_process` | `bool` | `false` | `false` = in-process `InMemoryGossipStore` (DashMap, zero network). `true` = chitchat gossip node + SWIM liveness over UDP. |
| `chitchat_listen_addr` | `Option<String>` | `None` | `host:port` the chitchat node binds and advertises. Required when `cross_process = true`. |
| `chitchat_seeds` | `Vec<String>` | `[]` | Peer `host:port`s to reach at startup. |
| `cluster_id` | `String` | `"springtale"` | Two nodes with different cluster ids won't peer. |
| `swim_listen_addr` | `Option<String>` | `None` (ephemeral loopback) | SWIM liveness bind. |
| `swim_seeds` | `Vec<String>` | `[]` | SWIM seeds to announce to at startup. |

Single-machine multi-process example:

```toml
[cooperation]
cross_process = true
chitchat_listen_addr = "127.0.0.1:9601"
chitchat_seeds = ["127.0.0.1:9602"]
swim_listen_addr = "127.0.0.1:9701"
swim_seeds = ["127.0.0.1:9702"]
```

See [`docs/guide/cooperation.md`](../guide/cooperation.md) for how the
gossip substrate feeds neighbour awareness.

---

## 10. Chat Connector Sections

Each is optional. If a section is absent, that connector is not loaded. All credential fields are `Secret<String>` — stored encrypted, never appearing in logs or API responses.

### 10.1 `[telegram]`

Telegram Bot API connector.

| Key | Type | Default | Description |
|---|---|---|---|
| `bot_token` | `Secret<String>` | (required) | Bot API token from @BotFather |
| `api_base` | `String` | `"https://api.telegram.org"` | Telegram API base URL |
| `update_mode` | `String` | `"polling"` | `"polling"` or `"webhook"` |
| `webhook_url` | `Option<String>` | `None` | Required when `update_mode = "webhook"` |
| `webhook_secret` | `Option<Secret<String>>` | `None` | Required when `update_mode = "webhook"`. Used to authenticate Telegram webhook callbacks via the `X-Telegram-Bot-Api-Secret-Token` header. |
| `poll_timeout` | `u64` | `30` | Long-polling timeout in seconds |

### 10.2 `[discord]`

Discord via `twilight-gateway`. **Warning:** Discord complies with government data requests. Workspace admins can silently read bot-accessible channels.

| Key | Type | Default | Description |
|---|---|---|---|
| `bot_token` | `Secret<String>` | (required) | Bot token (`NDc...`) |
| `application_id` | `u64` | (required) | Discord application ID |
| `guild_id` | `Option<u64>` | `None` | Guild ID for fast guild-scoped slash command registration. If absent, commands register globally. |
| `enable_message_content` | `bool` | `false` | Request the `MESSAGE_CONTENT` privileged intent. Discord transmits all watched-channel message content to the bot. |
| `enable_direct_messages` | `bool` | `false` | Enable DM triggers |
| `enable_reactions` | `bool` | `false` | Enable reaction triggers |
| `message_jitter_secs` | `u64` | `0` | Random 0..N second delay on sends |
| `commands` | `Vec<SlashCommandConfig>` | `[]` | Slash commands to register |

Each `SlashCommandConfig` has `name: String` and `description: String`.

### 10.3 `[slack]`

Slack Socket Mode + Web API. **Warning:** workspace admins can read every message, including DMs, with no notification.

| Key | Type | Default | Description |
|---|---|---|---|
| `bot_token` | `Secret<String>` | (required) | Bot user OAuth token (`xoxb-...`) |
| `app_token` | `Secret<String>` | (required) | App-level token (`xapp-...`) for Socket Mode |
| `message_jitter_secs` | `u64` | `0` | Random 0..N second delay on sends |

### 10.4 `[irc]`

Native IRC client.

| Key | Type | Default | Description |
|---|---|---|---|
| `server` | `String` | (required) | IRC server hostname |
| `port` | `u16` | `6697` | Server port |
| `use_tls` | `bool` | `true` | Use TLS. Must be `true` for production. |
| `nick` | `String` | (required) | Bot nickname |
| `nickserv_password` | `Option<Secret<String>>` | `None` | NickServ password |
| `sasl_enabled` | `bool` | `false` | Enable SASL PLAIN (required on Libera.Chat and similar) |
| `channels` | `Vec<String>` | `[]` | Channels to auto-join |
| `command_prefix` | `String` | `"!"` | Command prefix |
| `message_jitter_secs` | `u64` | `15` | Random 0..N second delay on sends, for social-graph obfuscation |

### 10.5 `[nostr]`

Nostr relays with NIP-44 encrypted DMs.

| Key | Type | Default | Description |
|---|---|---|---|
| `private_key` | `Secret<String>` | (required) | Private key, `nsec` bech32 or hex (secp256k1) |
| `relays` | `Vec<String>` | (required) | Relay URLs (at least one) |
| `dm_encryption` | `String` | `"nip44"` | DM encryption: `"nip44"` (modern) or `"nip04"` (legacy, deprecated) |
| `message_jitter_secs` | `u64` | `30` | Random 0..N second delay on sends |

### 10.6 `[signal]`

Bridges to a `signal-cli` daemon for Signal Protocol messaging.

| Key | Type | Default | Description |
|---|---|---|---|
| `daemon_url` | `String` | (required) | `signal-cli` daemon HTTP endpoint, e.g. `"http://localhost:8080"` |
| `account_id` | `String` | (required) | Account identifier (UUID or alias). **Not** the phone number, which lives in `signal-cli`'s own data directory. |
| `message_jitter_secs` | `u64` | `0` | Random 0..N second delay on sends |

### 10.7 `connector-matrix`

Not present in the workspace. `matrix-sdk` 0.16 pins `rusqlite` 0.37 which has CVE-2025-70873 (heap info disclosure). Springtale's store uses the patched `rusqlite` 0.39. The connector is held until `matrix-sdk` catches up.

---

## 11. Service Connector Sections

Non-chat connectors. Configured via the `[config.connector.{name}]` family of API endpoints or directly in `springtale.toml` under the section names below.

### 11.1 `[kick]`

Kick streaming platform, OAuth 2.1 PKCE.

| Key | Type | Default | Description |
|---|---|---|---|
| `client_id` | `String` | (required) | OAuth 2.1 client ID |
| `client_secret` | `Secret<String>` | (required) | OAuth 2.1 client secret |
| `redirect_uri` | `String` | (required) | OAuth redirect URI |
| `scopes` | `Vec<String>` | `["user:read","channel:read","channel:write","chat:write","events:subscribe"]` | OAuth scopes |
| `api_base` | `String` | `"https://api.kick.com"` | Kick API base |
| `oauth_base` | `String` | `"https://id.kick.com"` | Kick OAuth server base |
| `webhook_callback_url` | `Option<String>` | `None` | Webhook callback URL |

### 11.2 `[github]`

| Key | Type | Default | Description |
|---|---|---|---|
| `token` | `Secret<String>` | (required) | Personal Access Token |
| `webhook_secret` | `Option<Secret<String>>` | `None` | Webhook HMAC-SHA256 secret |
| `api_base` | `String` | `"https://api.github.com"` | GitHub API base |

### 11.3 `[bluesky]`

| Key | Type | Default | Description |
|---|---|---|---|
| `identifier` | `String` | (required) | Bluesky handle or DID |
| `password` | `Secret<String>` | (required) | Account password (app password recommended) |
| `pds_base` | `String` | `"https://bsky.social"` | ATProto PDS base |
| `jetstream_url` | `String` | `"wss://jetstream2.us-west.bsky.network/subscribe"` | Jetstream WebSocket URL |

### 11.4 `[presearch]`

| Key | Type | Default | Description |
|---|---|---|---|
| `api_key` | `Secret<String>` | (required) | Presearch API key |
| `api_base` | `String` | `"https://presearch.com"` | API base URL |
| `cache_ttl_secs` | `u64` | `300` | Cache TTL in seconds |
| `allowed_scrape_hosts` | `Vec<String>` | `[]` | Hosts allowed for post-search scraping |

### 11.5 `[http]`

Generic HTTP connector.

| Key | Type | Default | Description |
|---|---|---|---|
| `allowed_hosts` | `Vec<String>` | `[]` | Hostnames this connector may reach. Each becomes a `NetworkOutbound` capability. |
| `default_headers` | `HashMap<String, String>` | `{}` | Default headers attached to every request |
| `timeout_secs` | `u64` | `30` | Per-request timeout |

### 11.6 `[filesystem]`

| Key | Type | Default | Description |
|---|---|---|---|
| `watch_paths` | `Vec<PathBuf>` | `[]` | Paths to watch for `FileWatch` triggers |
| `read_paths` | `Vec<PathBuf>` | `[]` | Paths the connector may read |
| `write_paths` | `Vec<PathBuf>` | `[]` | Paths the connector may write |
| `debounce_ms` | `u64` | `500` | Debounce interval for watch events |

### 11.7 `[shell]`

| Key | Type | Default | Description |
|---|---|---|---|
| `allowed_commands` | `Vec<String>` | `[]` | Commands on the allow-list. Anything outside this list is rejected. |
| `timeout_secs` | `u64` | `30` | Per-command timeout |
| `working_directory` | `Option<String>` | `None` | Working directory for spawned commands |

### 11.8 `[browser]`

Headless Chromium.

| Key | Type | Default | Description |
|---|---|---|---|
| `allowed_domains` | `Vec<String>` | (required) | Hostnames the browser may navigate to. Each becomes a `NetworkOutbound` capability. |
| `chrome_path` | `Option<String>` | auto-detect | Path to Chrome/Chromium binary |
| `disable_telemetry` | `bool` | `true` | Launches Chromium with telemetry flags disabled |
| `message_jitter_secs` | `u64` | `0` | Random 0..N second delay on sends |

---

## 12. Example

```toml
# Minimal config — everything else defaults
heartbeat_interval_secs = 1800

[api]
bind = "127.0.0.1:8080"
rate_limit_per_sec = 100

[ai_ollama]
base_url = "http://127.0.0.1:11434"
model = "llama3.1:8b"

[telegram]
bot_token = "123456:ABC-..."
update_mode = "polling"

[bot]
context_window = 50
vault_timeout_secs = 300
persona.name = "Spring"
persona.tone = "neutral"
persona.prefix = "/"
```

---

## 13. Environment Overrides

Environment variables override file values. Prefix: `SPRINGTALE_`. Nesting separator: `__` (double underscore).

**TABLE II. COMMON OVERRIDES**

| Variable | Overrides |
|---|---|
| `SPRINGTALE_STORE__PATH` | `[store] path` |
| `SPRINGTALE_CRYPTO__VAULT_PATH` | `[crypto] vault_path` |
| `SPRINGTALE_TRANSPORT__SOCKET_PATH` | `[transport] socket_path` |
| `SPRINGTALE_API__BIND` | `[api] bind` |
| `SPRINGTALE_API__RATE_LIMIT_PER_SEC` | `[api] rate_limit_per_sec` |
| `SPRINGTALE_HEARTBEAT_INTERVAL_SECS` | `heartbeat_interval_secs` |
| `RUST_LOG` | Log level filter (e.g. `info`, `debug`, `springtaled=trace`) |

Priority (highest wins): environment variable → TOML file → built-in default.

### 13.1 Passphrase acquisition

The vault passphrase is acquired at boot via a 3-way fallback, in priority order:

1. **`SPRINGTALE_PASSPHRASE_FILE`** — path to a file containing the passphrase. This is the **Docker secrets** pattern and the recommended production option.
2. **`SPRINGTALE_PASSPHRASE`** — literal passphrase in the environment. **Dev only** — visible in `docker inspect`, process listings, and shell history.
3. **Interactive prompt** — if stdin is a TTY, prompt the user.

If none of the above succeed, boot fails.

---

## 14. Docker Environment

When running via Docker Compose, paths map to the `/data` volume:

```yaml
environment:
  - SPRINGTALE_PASSPHRASE_FILE=/run/secrets/vault_passphrase
  - SPRINGTALE_STORE__PATH=/data/springtale.db
  - SPRINGTALE_CRYPTO__VAULT_PATH=/data/vault.bin
  - SPRINGTALE_TRANSPORT__SOCKET_PATH=/data/springtale.sock
  - SPRINGTALE_API__BIND=0.0.0.0:8080
  - RUST_LOG=info

secrets:
  - vault_passphrase
```

The container mounts `./springtale.toml` read-only at `/etc/springtale/springtale.toml` and `./data` at `/data`.

---

## 15. Security Notes

- The API binds `127.0.0.1` by default. Binding `0.0.0.0` exposes the management API to the network — only do this behind a reverse proxy or in Docker with explicit intent.
- Prefer `SPRINGTALE_PASSPHRASE_FILE` (Docker secrets) over `SPRINGTALE_PASSPHRASE` (env). The latter leaks into `docker inspect`, logs, and process listings.
- Database file permissions are set to `0o600` on creation.
- All credential fields (`api_key`, `bot_token`, `private_key`, `sasl_password`, etc.) are wrapped in `Secret<String>` — they cannot be logged, cloned into unprotected memory, or serialized back out through the API.

---

## References

- [1] API endpoint reference: [api.md](api.md)
- [2] CLI reference: [cli.md](cli.md)
- [3] Docker deployment: [`../QUICKSTART.md`](../QUICKSTART.md) §3
- [4] Config loading: `apps/springtaled/src/config.rs`
- [5] Security posture: [`../arch/SECURITY.md`](../arch/SECURITY.md)
