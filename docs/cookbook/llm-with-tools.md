# Recipe: LLM with tools

Let the bot's AI call connector actions mid-conversation. All three
production AI adapters (Anthropic, Ollama, OpenAI-compat) support tool
calling, and `springtale-bot::tool_runner` routes every tool call back
through the same capability gate that guards direct dispatch.

Tool calling lives on the **bot chat path**, not in the rule engine.
There is no "AI with tools" rule action — when a chat message reaches
the bot and no deterministic command matches, the AI fallback runs, and
*that* invocation is allowed to call tools. Rules keep using the plain
`AiComplete` action (no tools).

## Basic shape

Configure the bot with an AI adapter and a tool policy:

```toml
[bot]
context_window = 50

[bot.persona]
name = "SupportBot"

# Which connector actions the AI may call.
[bot.tool_policy]
allow = [
  "connector-github__list_issues",
  "connector-github__get_issue",
  "connector-presearch__search",
]
max_iterations = 5
```

The AI adapter is not in the TOML. Set it per level — for the whole
colony here, `springtale config ai set --scope colony --type anthropic
--api-key-stdin` — or per formation / per agent, from the CLI, the
dashboard, or `POST /config/ai/configure`.

What happens:

1. A user DMs the bot on any chat connector (Telegram, Discord, the
   in-app chat panel, …).
2. The router finds no `/command` match, so the AI fallback runs.
3. The tool_runner enumerates callable tools from the connector
   registry (enabled connectors only), filters them through
   `tool_policy`, and calls the adapter's `complete_with_tools`.
4. If the model emits a tool call, the tool_runner executes it via
   `ConnectorRegistry::execute()` — the same path the rule engine
   uses, so capability checks, rate limits, and sandbox isolation
   apply automatically.
5. Results are fed back as `tool` role messages; the loop continues
   until the model produces plain text or hits the iteration cap.
6. The final text goes back to the user on the channel they wrote in.

## Tool naming convention

Tools are addressed as `{connector}__{action}` — double underscore:

- `connector-github__list_issues`
- `connector-presearch__search`
- `connector-filesystem__read_file`

The tool's JSON schema (input parameters, return shape) is
auto-generated from the action's declaration. The AI sees:

```json
{
  "name": "connector-github__list_issues",
  "description": "List issues for a repo",
  "parameters": {
    "type": "object",
    "properties": {
      "owner": {"type": "string"},
      "repo": {"type": "string"},
      "state": {"type": "string", "enum": ["open", "closed", "all"]}
    },
    "required": ["owner", "repo"]
  }
}
```

## The three policy modes

`[bot.tool_policy]` decides what the model can even see:

- **Default mode** (`allow` empty — ships this way): every `read_only`
  action on an enabled connector is callable. Zero side effects, so
  least-privilege holds out of the box. Mutating actions are invisible
  — unless `writes_with_approval = true`, which exposes them but
  routes **each call** through the blocking approval gate so you
  confirm from chat before anything fires.
- **Explicit mode** (`allow` non-empty): exactly the globs you list.
  `["connector-github__read_*", "connector-telegram__*"]` works.
- **`deny` always wins**, in both modes:

```toml
[bot.tool_policy]
deny = ["*__execute", "*__delete_*"]   # belt and braces
```

This is **defence in depth**. The connector's capability declaration
gates what it can do globally; the tool policy gates what this bot's
model can request; the approval gate fronts anything mutating. All
layers must allow for a call to succeed.

## Capability check

When the AI calls a tool, dispatch asks the sentinel:

1. Does this connector have the capability needed for this action?
2. Does the sentinel's circuit breaker allow it right now?
3. Does the rate limiter allow it?
4. Is the action destructive (or `ShellExec`)? If yes → approval gate.

The capability layer doesn't trust the AI. An AI calling
`connector-filesystem__write_file` against `/etc/passwd` is denied
because the path isn't on the filesystem allow-list, regardless of
what the prompt said.

If an approval is pending when the daemon restarts, the paused tool
loop is checkpointed (`tool_loop_checkpoints` table) and resumed after
the verdict — replaying exactly the persisted calls, never re-derived
ones.

## Session memory

The conversation is keyed to the chat session. Subsequent messages in
the same channel see prior turns via the bot's session memory
(`bot_memory` table, encrypted at rest). Memory is bounded by
`[bot] context_window` (default 50 turns); older turns drop off.

## Tool failures

If a tool call fails (network error, capability denied, timeout), the
failure is fed back to the AI as a tool result with an error field.
The AI can retry, fall back to a different tool, or explain to the
user.

## Cost control

Tool calling iterates — each round is one full AI roundtrip.
`max_iterations` caps this (0 = default 5: enough for two-hop "look up
then send" flows, tight enough to make runaway loops visible). Past
the cap, the model's last text stands. Tool outputs are truncated to
8 KiB before being fed back so an oversized payload can't blow the
context.

On top of that, the guardrail layer enforces `[sentinel]
daily_token_limit` — once a bot crosses its daily token cap, further
AI calls are denied until the UTC day rolls over.

## Audit

Every tool dispatch runs through the sentinel, so it lands in the
audit trail like any other action:

```sql
SELECT * FROM audit_trail
WHERE created_at > datetime('now', '-1 hour')
ORDER BY created_at;
```

You'll see one row per tool dispatch with the connector and action.

## Gotchas

- **The AI sees tool descriptions, not implementations.** Write good
  action descriptions in your connector — the AI uses them to decide
  whether to call a tool.
- **Mutating tools route through the approval gate.** A support bot
  asked to "delete the user's account" hits the gate. Headless
  installs default-deny.
- **Token usage scales with tool roundtrips.** Each iteration sends
  the full conversation back to the AI.
- **Streaming + tool calling is mode-dependent.** Anthropic streams
  text deltas during normal responses; tool calls arrive non-streamed.
  OpenAI-compat streams text but buffers tool-call argument JSON until
  complete. Plan for non-streamed bursts inside an otherwise-streaming
  response.
- **Rules don't get tools.** `Action::AiComplete` is plain completion.
  If you need "fetch then act" inside a rule, chain `RunConnector`
  steps — deterministic, auditable, no model in the loop.
