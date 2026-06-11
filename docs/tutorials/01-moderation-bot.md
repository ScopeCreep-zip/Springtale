# Tutorial 01: Telegram moderation bot

You're going to build a Telegram group moderation bot. It watches a
group for messages containing patterns you choose (slurs, doxxing
patterns, spam phrases), takes a first-level action (delete + warn),
and escalates to a human moderator on a second offence from the same
user.

By the end you'll have used: connectors, rule conditions, the bot
runtime's command router, session memory, and the moderation-grade
sentinel audit trail.

**Time:** 45 minutes if Telegram setup is new to you, 20 if you've
done it before.

**You'll need:**

- A Telegram account.
- A Telegram group you can add a bot to (or DM-only is fine if you
  just want to test the mechanics).
- Springtale installed and `springtale init` run. If you haven't:
  [`docs/QUICKSTART.md`](../QUICKSTART.md).

## Step 1 — Create a Telegram bot

In Telegram, message [@BotFather](https://t.me/BotFather):

```
/newbot
> What's the name?       MyMod
> What's the username?   my_mod_test_bot
```

BotFather replies with a token shaped like
`123456789:ABCdefGHIjklMNOpqrsTUVwxyzABCdefGHIjklMNO`. **Don't paste
it anywhere yet.** We'll put it in the Springtale vault, not in a
file.

If you want this bot in a group, add it now via the group's members
panel. Make it an admin so it can delete messages.

By default, bots in Telegram groups can't see message content (the
"privacy mode" setting). For moderation, you need it off:

```
/setprivacy
> Choose a bot          MyMod
> Privacy mode          Disable
```

## Step 2 — Scaffold the project

```bash
springtale-cli new telegram-bot
# Creating telegram-bot project in ~/.local/share/springtale/projects/telegram-bot-20260610-091500
cd ~/.local/share/springtale/projects/telegram-bot-*/
```

The CLI picks the destination itself (under the data directory's
`projects/`, timestamped) — you never pass a path, which is what keeps
the scaffold free of path-traversal surprises. You now have:

```
telegram-bot-<timestamp>/
├── springtale.toml          # daemon config for this project
└── rules/
    └── welcome.toml         # starter rule, fires on /start
```

Take a look:

```bash
cat springtale.toml
cat rules/welcome.toml
```

The scaffolded `welcome.toml` is enough to verify the bot is wired —
we'll add real moderation rules in step 4.

## Step 3 — Put the token in the vault

```bash
springtale-cli vault set telegram.bot_token
```

This prompts for the token (paste it; stdin only, never argv, never
env). The token is encrypted at rest using your vault passphrase
from the `init` step and isn't readable except by the running daemon.

Now start the daemon:

```bash
springtale-cli server start
```

In another terminal, verify the connector loaded:

```bash
springtale-cli connector list
```

You should see `connector-telegram` with status `Active`. If you see
`Pending` or `Error`, run `springtale-cli doctor` for diagnostics.

## Step 4 — Write the first moderation rule

Save this as `rules/flag-slurs.toml`:

```toml
[rule]
name = "flag-slurs"
description = "Match common slurs, delete + warn user, log to audit."
enabled = true

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "message_received"

[[conditions]]
type = "Regex"
field = "trigger.text"
# This pattern is illustrative.  Use a real curated list scoped to
# your community's standards — slurs change over time and across
# subcultures.  Maintain the list in a separate file so you can
# update it without touching the rule.
pattern = "(?i)\\b(slur1|slur2|slur3)\\b"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "delete_message"

[actions.params]
chat_id = "${trigger.chat.id}"
message_id = "${trigger.message_id}"

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${trigger.chat.id}"
text = "@${trigger.sender.username} — that message was removed per group rules.  This is your first warning."
```

Load it:

```bash
springtale-cli rule add rules/flag-slurs.toml
```

The daemon parses the TOML, validates the connector + event + action
exist, and stores the rule. It's now live.

Verify:

```bash
springtale-cli rule list
```

You should see two rules now: the scaffolded welcome and your new
`flag-slurs`.

## Step 5 — Test it

In your test Telegram group (or in a DM to the bot), send a message
containing one of your placeholder slurs (`slur1`, `slur2`, etc.).
Expected:

1. The message is deleted within a couple of seconds.
2. The bot replies with the warning text.
3. `springtale-cli events --limit 5` shows the trigger fire + the two
   action dispatches.
4. The `audit_trail` table has a `Go` verdict from sentinel for each
   dispatched action.

If step 1 doesn't happen, check:

- Bot is an admin in the group (admins can delete messages; regular
  members can't).
- Privacy mode is disabled (`/setprivacy` with BotFather).
- The pattern actually matches your test message (try
  `springtale-cli trace --connector connector-telegram` and watch
  events fire in real time).

## Step 6 — Add the escalation logic

Single-strike moderation is too blunt. Let's make a second offence
from the same user escalate to a human moderator.

We'll use **session memory**: the bot runtime stores per-user state
across messages. We'll bump a counter in the user's session each
time they trip the rule.

Save as `rules/escalate-on-second-offence.toml`:

```toml
[rule]
name = "escalate-on-second-offence"
description = "If a user trips flag-slurs twice, DM the moderator."
enabled = true

[trigger]
type = "InternalEvent"
event = "rule_fired"

[[conditions]]
type = "FieldEquals"
field = "trigger.rule_name"
value = "flag-slurs"

# The bot runtime exposes session memory.  We're going to read the
# user's offence count, increment it, and write it back.
[[actions]]
type = "Transform"
op = "IncrementSession"

[actions.params]
session_key = "telegram:${trigger.payload.sender.username}"
counter = "slur_offences"

[[conditions]]
type = "FieldGreaterThanOrEqual"
field = "transform.slur_offences"
value = 2

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "MODERATOR_CHAT_ID_HERE"   # replace with your DM chat id
text = "⚠️ @${trigger.payload.sender.username} tripped flag-slurs ${transform.slur_offences} times in ${trigger.payload.chat.title}.  Last message: \"${trigger.payload.text}\""
```

Replace `MODERATOR_CHAT_ID_HERE` with your own Telegram user ID.
Easiest way to find it: DM your bot any message and inspect
`springtale-cli events --limit 1`. The `chat.id` from your DM is
your user ID.

Load it:

```bash
springtale-cli rule add rules/escalate-on-second-offence.toml
```

## Step 7 — Test escalation

Trip the moderation rule twice from the same user in your test group.
Expected:

- First offence: message deleted, warning posted in group. No DM to
  moderator.
- Second offence: message deleted, warning posted in group, **plus**
  a DM to the moderator with the user's offence count.

## What you just learned

| Concept | Where it appeared |
|---|---|
| Connector configuration via vault | Step 3 — `vault set` |
| Rule structure (trigger / conditions / actions) | Step 4 |
| Variable interpolation (`${trigger.…}`) | Step 4 actions |
| Regex conditions | Step 4 — `(?i)\b…\b` pattern |
| Session memory | Step 6 — `IncrementSession` |
| Rule chaining via `InternalEvent` | Step 6 — `event = "rule_fired"` |
| Sentinel audit trail | Step 5 — `audit_trail` table |

## Extend this

| Add | See |
|---|---|
| Toxic-pattern updates over time (don't hardcode) | [cookbook: pattern lists from files](../cookbook/scheduled-tasks.md) |
| Different actions per platform | [cookbook: event fan-out](../cookbook/event-fan-out.md) |
| AI-assisted classification ("is this hate speech?") | [cookbook: LLM with tools](../cookbook/llm-with-tools.md) |
| A formation that watches multiple chats with shared state | [tutorial 03](03-llm-research-swarm.md) |
| Approve-before-action workflow instead of auto-delete | [guide/security.md §7 — approval gate](../guide/security.md) |

## Safety notes

- **Don't auto-ban based on a single pattern match.** False positives
  in moderation are unrecoverable for the user; build your escalation
  to favour human review.
- **Don't log moderated message content.** This bot already deletes
  the message and surfaces it to the moderator; logging the content
  long-term creates a target dataset. Mental model + audit trail are
  fine because they're scoped + retained on policy.
- **Test rule changes against a staging chat first.** A bug in your
  regex can mass-delete in your real group. Use a test group with
  bot + you + your alt account.

## Cleanup

If you were just trying this out:

```bash
springtale-cli rule delete flag-slurs
springtale-cli rule delete escalate-on-second-offence
springtale-cli rule delete welcome
springtale-cli connector remove connector-telegram
springtale-cli vault unset telegram.bot_token
```

Delete the bot via BotFather (`/deletebot`).
