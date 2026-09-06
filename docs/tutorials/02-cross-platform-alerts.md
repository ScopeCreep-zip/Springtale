# Tutorial 02: Cross-platform stream alerts

When a Kick stream goes live, your audience may be spread across
Bluesky, Discord, and Telegram. You're going to build a single rule
that:

- Watches Kick for stream-live events.
- Posts a Bluesky note announcing the stream.
- Pings a Discord channel.
- DMs a Telegram subscriber list.

This is the **multi-connector fan-out** shape — one trigger, N
parallel actions. No agents, no cooperation, just a wide rule.

**Time:** 30 minutes (longer if you have to set up Bluesky / Discord
/ Telegram from scratch).

**You'll need:**

- Springtale installed and `springtale init` run.
- A Kick account with at least one stream you can start (or pick a
  channel you don't own and use the `channel_followed` trigger
  instead — same shape, different trigger).
- A Bluesky account.
- A Discord server you can install a bot in.
- A Telegram bot (see [tutorial 01](01-moderation-bot.md) for setup).

## Step 1 — Set up the connectors

We need four connectors. All four take their credentials through the
connector setup flow — each connector's card in the dashboard, or the
connector setup API. Kick's is an OAuth 2.1 PKCE flow the card walks you
through; the other three want a token you already hold:

- Bluesky: ATProto session — handle (`your.handle.bsky.social`) plus an
  app password from https://bsky.app/settings/app-passwords
- Discord: bot token from https://discord.com/developers/applications
- Telegram: token from BotFather, same as tutorial 01

Tokens never go through argv, env, or the TOML.

Verify all four loaded:

```bash
springtale-cli server start &
sleep 2
springtale-cli connector list
```

You should see four connectors with status `Active`.

## Step 2 — The fan-out rule

Save as `rules/stream-live-fanout.toml`:

```toml
[rule]
name = "stream-live-fanout"
description = "When my Kick stream goes live, announce on Bluesky + Discord + Telegram."
enabled = true

[trigger]
type = "ConnectorEvent"
connector = "connector-kick"
event = "stream_live"

# Optional: only fire for *your* channel.  The trigger payload
# includes broadcaster.username; filter on it so you don't get
# pinged for everyone you follow going live.
[[conditions]]
type = "FieldEquals"
field = "trigger.broadcaster.username"
value = "your_kick_username"

# Bluesky post.
[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "create_post"

[actions.params]
text = "🔴 LIVE on Kick — ${trigger.title}\n\nhttps://kick.com/${trigger.broadcaster.username}"

# Discord ping.
[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_message"

[actions.params]
channel_id = "YOUR_DISCORD_CHANNEL_ID"
text = "@everyone — going live now: ${trigger.title}\nhttps://kick.com/${trigger.broadcaster.username}"

# Telegram DM.  For a real subscriber list, use Chain to iterate.
# For one DM (or a Telegram channel), just hardcode the chat_id.
[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "YOUR_TELEGRAM_CHANNEL_ID"
text = "🔴 LIVE on Kick — ${trigger.title}\n\nhttps://kick.com/${trigger.broadcaster.username}"
```

Replace the channel IDs / username with yours, then load:

```bash
springtale-cli rule add rules/stream-live-fanout.toml
```

## Step 3 — Test it

Two ways:

**Real test:** Start a stream on Kick. Within a few seconds you
should see the post on Bluesky, the ping in Discord, the message in
Telegram.

**Synthetic test:** `springtale-cli rule run stream-live-fanout`
fires the rule against a synthetic payload, which lets you verify
the action dispatches succeed without actually streaming. The
synthetic payload has placeholder values; check the dispatched
content shows the placeholder text.

Inspect what happened:

```bash
springtale-cli events --limit 10
springtale-cli trace --rule stream-live-fanout
```

`trace` is real-time. Keep it open while you stream.

## Step 4 — Add idempotency

Kick's `stream_live` trigger fires every time the stream toggles to
live. If your stream drops and reconnects, you'll fan out twice.

Add a cooldown via session memory:

```toml
# Append to rules/stream-live-fanout.toml, at the top of [[actions]]:

[[conditions]]
type = "And"

[[conditions.children]]
type = "FieldEquals"
field = "trigger.broadcaster.username"
value = "your_kick_username"

[[conditions.children]]
type = "SessionFieldOlderThan"
session_key = "kick-live-cooldown"
field = "last_fanout_at"
duration = "30m"
```

And add a session-write action **before** the platform fan-outs:

```toml
[[actions]]
type = "Transform"
op = "SetSessionField"

[actions.params]
session_key = "kick-live-cooldown"
field = "last_fanout_at"
value = "${now}"
```

Now if Kick fires `stream_live` twice within 30 minutes, the second
firing finds `last_fanout_at` < 30 minutes old and the condition
short-circuits. Fan-out happens once per stream session.

## Step 5 — Per-platform formatting

You probably want each platform's message to read differently:

- **Bluesky**: 300-character limit, hashtag-friendly.
- **Discord**: rich embed, image, link preview.
- **Telegram**: long-form OK, supports Markdown.

Switch from a single `text` field to per-action formatting:

```toml
# Bluesky — short, hashtag-friendly:
[actions.params]
text = "🔴 Live: ${trigger.title} — kick.com/${trigger.broadcaster.username} #${trigger.category}"

# Discord — embed:
[[actions]]
type = "RunConnector"
connector = "connector-discord"
action = "send_embed"

[actions.params]
channel_id = "YOUR_DISCORD_CHANNEL_ID"
title = "${trigger.title}"
description = "${trigger.broadcaster.username} is live on Kick"
url = "https://kick.com/${trigger.broadcaster.username}"
color = 0xFF0000
image = "${trigger.thumbnail_url}"

# Telegram — Markdown:
[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "YOUR_TELEGRAM_CHANNEL_ID"
text = "🔴 *${trigger.title}*\n\n[Watch on Kick](https://kick.com/${trigger.broadcaster.username})"
parse_mode = "Markdown"
```

## What you just learned

| Concept | Where it appeared |
|---|---|
| OAuth setup via `connector setup` | Step 1 — Kick |
| Multi-connector single rule | Step 2 — three platform actions |
| Trigger payload filtering | Step 2 — `FieldEquals` on broadcaster |
| Variable interpolation across services | Step 2 — `${trigger.title}` everywhere |
| Synthetic rule runs | Step 3 — `rule run …` |
| Idempotency via session memory | Step 4 — cooldown |
| Per-platform formatting | Step 5 — embed vs Markdown vs plain |

## Extend this

| Add | See |
|---|---|
| Subscriber list (DM N people, not just a channel) | [cookbook: event fan-out](../cookbook/event-fan-out.md) |
| Conditional output (different message for different stream categories) | [guide/rules.md](../guide/rules.md) — `[[conditions]]` with `Or` |
| Per-platform retry on failure | [guide/rules.md](../guide/rules.md) — retry policy on actions |
| Tracking which platforms succeeded vs failed | `springtale-cli events --connector …` |
| Tying this into a moderation bot | [tutorial 01](01-moderation-bot.md) |

## Safety notes

- **The fan-out rule has access to every platform's send capability.**
  Sentinel logs each dispatched action; review the audit trail
  periodically.
- **One compromised connector ≠ all compromised.** Each connector's
  credentials are isolated in the vault; a Kick OAuth refresh failure
  doesn't expose your Bluesky app password.
- **Rate limits apply per-platform.** If you fan out to many
  subscribers, you'll hit Telegram's 30/sec or Discord's 50/sec
  bursts. The scheduler retries with exponential backoff — see
  [guide/rules.md](../guide/rules.md).

## Cleanup

```bash
springtale-cli rule delete stream-live-fanout
springtale-cli connector remove connector-kick
springtale-cli connector remove connector-bluesky
springtale-cli connector remove connector-discord
springtale-cli connector remove connector-telegram
```
