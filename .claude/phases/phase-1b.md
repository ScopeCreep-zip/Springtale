# Phase 1b — Bot Foundations

> Source: `docs/current-arch/ARCHITECTURE.md` §3, §6.9, §14.1-14.2
> Depends on: Phase 1a complete

## Goal

Classical bot runtime with deterministic command routing. First chat
connector (Telegram). No AI. Users talk to the bot through Telegram and
get the same automation they configured in Phase 1a, plus interactive
commands.

## Two Sub-Milestones

**1b-i: Bot core** — testable without any chat platform.
**1b-ii: First chat connector** — Telegram makes the bot reachable.

## Milestone 1b-i: springtale-bot

The bot runtime lives in `crates/springtale-bot/`. It depends on core,
crypto, connector, store, and transport.

**How the event loop works:**

```rust
pub async fn run(bot: &Bot) -> Result<()> {
    loop {
        tokio::select! {
            Some(msg) = bot.connector_events.recv() => {
                // Route through command router
                // On error: log + continue (NEVER propagate ? — kills the loop)
            }
            Some(trigger) = bot.rule_events.recv() => {
                // Evaluate and dispatch through rule engine
            }
            Some(job) = bot.job_events.recv() => {
                // Execute scheduled job
            }
        }
    }
}
```

**Critical design detail:** The event loop must NOT use `?` on individual
message processing. A single bad message must not crash the bot. Log the
error, continue to next event. This was flagged in the architecture audit.

**Command router (no AI):**
- `router::prefix` — exact prefix match: `/search`, `/help`, `/remind`, `/status`
- `router::pattern` — regex and keyword matching for non-slash triggers
- `router::alias` — user-defined aliases persisted in SQLite (e.g., `/s` -> `/search`)
- `router::fallback` — no match returns "Unknown command. Try /help" (AI fallback added in Phase 2a)
- Router is a pure function: `(message_text) -> RouteResult::Command(name, args) | RouteResult::NoMatch`

**Handler dispatch:**
- `handler::registry` — HashMap of command name -> handler function
- `handler::builtin` — `/help` (list commands), `/status` (bot health), `/rules` (list active rules), `/connectors` (list installed)
- `handler::connector` — generic handler: connector name + action derived from command. When connector-presearch is installed, `/search <query>` auto-registers.

**How auto-registration works:** When a connector is installed via
`springtale connector install`, the bot reads its `actions()` and
registers each as a command. `connector-presearch` with action `search`
becomes `/search`. `connector-github` with action `create_issue` becomes
`/github create_issue`. Users create aliases for convenience.

**Conversation state:**
- `state::session` — per `(user_id, channel_id)` key in SQLite. Tracks what the bot last said, what it's waiting for (multi-step flows). Row-level isolation: no cross-user state access.
- `state::prefs` — user preferences: timezone, language, notification settings. Persisted in SQLite. User manages via `/prefs set timezone America/New_York`.
- `state::persona` — bot persona config: name, response tone, template library. Loaded from `springtale.toml`. Not user-configurable (admin sets it).

**Persistent memory:**
- `memory::persistent` — SQLite-backed. Structured typed schemas, not arbitrary strings. Encrypted at rest via vault.
- `memory::context` — sliding window of recent conversation per user. Configurable size (default: 50 messages).
- `memory::compaction` — Phase 1b: simple truncation (drop oldest). Phase 2a: AI summarization when adapter is plugged in.

**Bot identity:**
- `identity::bot_id` — Ed25519 keypair generated via springtale-crypto. Stored in vault.
- Phase 1b: keypair is the bot's identity. Simple.
- Phase 3: HKDF derives per-community pseudonyms from this keypair for Rekindle integration. The derivation code is not written yet — just the keypair.

**Safety features (from audit §2.8 IPV threat model):**
- No OS notifications by default. Connector events and bot responses do not trigger push notifications unless user explicitly enables via `/prefs set notifications on`.
- Vault auto-lock after configurable inactivity (default: 5 min). CLI prompts for passphrase to resume. Tauri modal in Phase 2b.

**Integration with springtaled:**
- `springtaled` startup adds a new step between scheduler start and API start: initialize `springtale-bot` with references to connector registry, rule engine, scheduler, and store
- Bot event loop runs as a tokio task alongside existing scheduler tasks
- The rule engine from Phase 1a runs INSIDE the bot's event loop — cron rules, webhook rules, filesystem rules all fire through the same dispatch path

**Testing (without Telegram):**
- Headless integration test: fire a `Trigger::ConnectorEvent` -> bot routes -> handler executes -> response generated
- Command routing tests: prefix match, pattern match, alias resolution, fallback
- Session isolation tests: two users send commands concurrently, state does not leak
- Memory compaction test: fill to N+1, verify oldest dropped

## Milestone 1b-ii: connector-telegram

Standard connector structure per `.claude/rules/connector-guidelines.md`.

**How to build:**

Telegram Bot API client. Two modes of receiving updates:
1. **Webhook mode:** Telegram sends HTTPS POST to your endpoint. Requires public URL. Uses the management API's webhook endpoint: `POST /webhook/connector-telegram/message_received`.
2. **Long-polling mode:** Bot calls `getUpdates` in a loop. Works behind NAT. Default for dev/self-hosted.

Mode selected in connector config. Long-polling is default.

**Auth:**
- Bot token from BotFather, stored as `Secret<String>` in connector config
- Webhook signature verification not natively supported by Telegram Bot API (unlike GitHub). Instead: use secret path token in webhook URL as auth.

**Connector trait implementation:**
- `triggers()`: `message_received`, `command_received` (filtered by /prefix)
- `actions()`: `send_message`, `send_photo`, `edit_message`, `delete_message`, `send_inline_keyboard`
- `execute("send_message", { chat_id, text, parse_mode })` -> call Telegram `sendMessage` API
- `on_event("message_received", handler)` -> handler called for every incoming message

**Typed API client:**
- `client::api.rs` — reqwest client to `https://api.telegram.org/bot{token}/`
- All request/response types as Rust structs with serde
- Methods: `send_message`, `send_photo`, `edit_message_text`, `delete_message`, `get_updates`, `set_webhook`, `delete_webhook`
- Inline keyboard builder for interactive buttons
- Message formatting: MarkdownV2 and HTML modes

**Research needed:** Telegram Bot API docs (https://core.telegram.org/bots/api).
Update polling semantics (`offset` parameter for confirming received updates).
MarkdownV2 escaping rules (Telegram has unusual escaping requirements).
Rate limits (30 messages/second to same chat, 20 messages/minute to same group).

**Integration:**
- Install: `springtale connector install ./connector-telegram.toml`
- Bot auto-registers Telegram message handler in event loop
- User sends `/search tokyo weather` in Telegram chat
- Bot receives via connector-telegram `on_event`
- Router matches `/search` prefix -> SearchHandler
- Handler calls `connector-presearch.execute("search", { query: "tokyo weather" })`
- Result formatted via response template
- Handler calls `connector-telegram.execute("send_message", { chat_id, text: formatted_result })`
- User sees result in Telegram

**Testing:**
- Mock Telegram API server (axum test server returning canned responses)
- Test: send_message serialization matches Telegram API format
- Test: getUpdates polling loop handles timeout, empty response, error response
- Integration test: message received -> routed -> connector called -> reply sent

## Not In Phase 1b

- No AI fallback parser (Phase 2a — router returns "unknown command" for now)
- No AI-powered memory compaction (Phase 2a — truncation only)
- No HKDF pseudonym derivation on BotId (Phase 3)
- No recursive pipeline orchestration (Phase 2a)
- No sub-agent spawning (Phase 2a)
- No sentinel behavioral monitor (Phase 2a)
- No additional chat connectors beyond Telegram (Phase 2a)
- No ATProto bridge (Phase 2a)
- No Rekindle bridge (Phase 3)
