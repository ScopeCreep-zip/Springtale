# Recipe: LLM with tools

An LLM action where the model can call connectors mid-response. Three
of the AI adapters (Anthropic, Ollama, OpenAI-compat) support tool
calling. The cooperation framework's `springtale-bot::tool_runner`
routes tool calls back through the same capability gate that guards
direct dispatch.

## Basic shape

```toml
[rule]
name = "support-bot-with-tools"
enabled = true

[trigger]
type = "ConnectorEvent"
connector = "connector-telegram"
event = "message_received"

[[conditions]]
type = "FieldEquals"
field = "trigger.chat.type"
value = "private"           # only DMs

[[actions]]
type = "AiCompleteWithTools"

[actions.params]
prompt = """
You are a support bot for a privacy-respecting automation platform.
You have access to tools to look up the user's account state.  Use
them when needed; respond directly when not.
"""
user_message = "${trigger.text}"
session_key = "telegram:${trigger.sender.username}"
tools = [
  "connector-github::list_issues",
  "connector-github::get_issue",
  "connector-presearch::search",
]
max_tool_iterations = 5

[[actions]]
type = "RunConnector"
connector = "connector-telegram"
action = "send_message"

[actions.params]
chat_id = "${trigger.chat.id}"
text = "${actions.0.response}"
```

What happens:

1. User sends a message in DM.
2. The `AiCompleteWithTools` action invokes the configured AI adapter
   with the prompt + user message + the listed tools.
3. The AI may decide to call a tool. If so, the tool_runner
   intercepts the tool call, dispatches it through the connector
   layer (with full capability check), and returns the result to the
   AI.
4. The AI may call more tools (up to `max_tool_iterations`).
5. The final text response is returned as `${actions.0.response}`.
6. The Telegram action posts the response.

## Tool naming convention

Tools are addressed as `connector-name::action-name`. Example:

- `connector-github::list_issues`
- `connector-presearch::search`
- `connector-filesystem::read_file`

The tool's JSON schema (input parameters, return shape) is
auto-generated from the action's declaration in the connector
manifest. The AI sees:

```json
{
  "name": "connector-github::list_issues",
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

## Capability check

When the AI calls a tool, the tool_runner asks the sentinel:

1. Does this connector have the capability needed for this action?
2. Does the sentinel's circuit breaker allow it right now?
3. Does the rate limiter allow it?
4. Is the action classified as destructive? If yes → approval gate.

The capability layer doesn't trust the AI. An AI calling
`connector-filesystem::write_file` against `/etc/passwd` will be
denied because the path isn't on the filesystem allow-list, regardless
of what the prompt said.

## Per-tool allow-list

The `tools` field in the action restricts which tools the AI can
call within this specific rule's invocation. Even if
`connector-github` has many actions, only the ones you list are
available.

The fan-out shape:

```toml
tools = [
  "connector-github::list_issues",
  "connector-github::get_issue",
  # NOT: "connector-github::create_issue" — read-only support bot
]
```

This is **defence in depth**. The connector's capability declaration
gates what it can do globally; the rule's `tools` list gates what
this specific LLM invocation can request. Both layers must allow for
the call to succeed.

## Streaming

If your AI adapter supports streaming (all three of ours do, including
OpenAI-compat after the G-series fixes), the response can stream
back to the user as it arrives:

```toml
[[actions]]
type = "AiCompleteWithTools"
streaming = true               # response yields tokens incrementally

[actions.params]
# ... same as before ...

[[actions]]
type = "StreamingResponse"
target = "connector-telegram"
action = "send_message"
edit_existing = true           # update the same message as tokens arrive
chunk_size = 100               # send updates every N tokens

[actions.params]
chat_id = "${trigger.chat.id}"
```

Telegram doesn't natively stream, but Telegram's `editMessageText`
lets us update the same message every N tokens, giving a streaming-
feeling output.

## Session memory

`session_key` ties the conversation to a session. Subsequent calls
with the same session key see prior messages via the bot's session
memory (`bot_memory` table). This is how the bot "remembers" who
you are across messages.

Memory is bounded by `[bot] context_window` (default 50 turns). Older
turns are dropped when the limit is hit.

## Tool failures

If a tool call fails (network error, capability denied, timeout),
the failure is fed back to the AI as a tool result with an error
field. The AI can retry, fall back to a different tool, or give up
and explain to the user.

## Cost control

Tool calling can iterate. Each iteration is one full AI roundtrip.
`max_tool_iterations` caps this — past the cap, the AI's final
response is whatever it has so far.

For paid APIs (Anthropic, OpenAI), this matters. Cost = N tokens *
M roundtrips. Set `max_tool_iterations = 3` for cheap support bots,
~10 for research workflows.

## Audit

Every tool call is logged:

```sql
SELECT * FROM audit_trail
WHERE rule_id = (SELECT id FROM rules WHERE name = 'support-bot-with-tools')
  AND created_at > datetime('now', '-1 hour')
ORDER BY created_at;
```

You'll see one row per AI invocation plus one row per tool dispatch.

## Gotchas

- **The AI sees tool descriptions, not implementations.** Write good
  action descriptions in your connector manifest — the AI uses them
  to decide whether to call a tool.
- **Destructive tools route through the approval gate.** A support
  bot asking the AI to "delete the user's account" hits the gate.
  Headless installs default-deny.
- **Token usage scales with tool roundtrips.** Each iteration sends
  the full conversation back to the AI. For long sessions with
  many tool calls, costs accumulate.
- **Streaming + tool calling is mode-dependent.** Anthropic streams
  text deltas during normal responses; tool calls arrive
  non-streamed. OpenAI-compat streams text but tool-call argument
  JSON is buffered until complete. Plan for non-streamed bursts
  inside an otherwise-streaming response.
- **Tool result size matters.** A tool returning 100 KB of JSON
  blows up the next AI call's context. Truncate or summarize before
  feeding back.
