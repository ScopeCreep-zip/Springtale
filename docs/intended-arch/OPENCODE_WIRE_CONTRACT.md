# OpenCode Integration — Wire Contract

**Status:** Design spec · **Updated:** 2026-04-17
**Companion to:** `ARCHITECTURE.md`, `COOPERATION.md`, `COOPERATION_IMPLEMENTATION_PLAN.md`
**Implements:** Springtale-as-an-agent-inside-OpenCode. One formation of bots per OpenCode turn instead of one LLM call.

---

## 0. Scope

This doc is the **executable wire contract** for making Springtale appear as a selectable agent inside [OpenCode](https://opencode.ai). It locks down every byte of the HTTP/SSE exchange between OpenCode's TUI and the Springtale daemon so implementation has no ambiguity.

**In scope.**
- Transport, auth, and header conventions
- Exact request and response schemas
- Tool-call streaming semantics
- Session identity and persistence
- Slash-command routing
- Cancellation, errors, edge cases
- Formation-event → SSE-chunk mapping
- Shipped config files the user drops into their repo

**Out of scope (covered elsewhere).**
- Formation internals — see `COOPERATION.md`
- Bot mapping to formation members — see `COOPERATION_IMPLEMENTATION_PLAN.md`
- Connector security model — see `SECURITY.md`

**Rule:** every claim in this doc is cited to a source file + line number in either `vercel/ai` (Vercel AI SDK, main branch) or `anomalyco/opencode` (OpenCode, dev branch). If something is not cited, it's a design decision owned by this doc and marked `[DECISION]`.

---

## 1. Repo facts

- OpenCode canonical repo: [`github.com/anomalyco/opencode`](https://github.com/anomalyco/opencode). The older `github.com/sst/opencode` returns `HTTP 301` to the same tree — same code, transferred from SST to Anomaly Innovations. Not a divergent fork.
- OpenCode consumes its language models through Vercel AI SDK's [`@ai-sdk/openai-compatible`](https://github.com/vercel/ai/tree/main/packages/openai-compatible) provider. Registered at `packages/opencode/src/provider/provider.ts:94-117`.
- Every OpenAI request/response shape in this doc is enforced by the AI SDK's schemas, NOT by OpenCode directly. That means the contract is portable to any Vercel-AI-SDK consumer, not OpenCode-specific.

---

## 2. Architecture at a glance

```
┌─────────────────────────────────────────────────────────────────┐
│                         OpenCode TUI                             │
│                                                                  │
│   User input       [agent: @springtale]    tools enabled:       │
│   "fix login bug"                           [read][edit][bash]  │
│                                                                  │
└──────────┬──────────────────────────────────────────────────────┘
           │  POST /v1/chat/completions
           │  Authorization: Bearer <SPRINGTALE_TOKEN>
           │  x-session-affinity: <sess>
           │  Content-Type: application/json
           │  Accept: text/event-stream
           ▼
┌─────────────────────────────────────────────────────────────────┐
│                 springtaled (management API)                     │
│                                                                  │
│   crate: springtale-opencode                                    │
│     ├── router — axum Router mounted at /v1                     │
│     ├── translate — OpenAI request → Formation intent          │
│     ├── stream — Formation events → OpenAI SSE chunks          │
│     ├── tools — tool_call emit + round-trip tracking            │
│     └── session — x-session-affinity → FormationId persistence  │
│                                                                  │
└──────────┬──────────────────────────────────────────────────────┘
           │  spawn_formation(intent, caps, workdir=cwd)
           ▼
┌─────────────────────────────────────────────────────────────────┐
│            Formation (crates/springtale-bot/cooperation)         │
│                                                                  │
│   momentum ticks → member AI stream → tool_call decision        │
│   consensus on destructive actions → emit on SSE                 │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
           │  SSE stream (data: chunks back to OpenCode)
           ▲
           │  On tool_call: OpenCode runs the tool LOCALLY with
           │  its own permission UX, sends role:"tool" result
           │  in next POST.
```

---

## 3. Transport

### 3.1 HTTP surface

- **Endpoint:** `POST {baseURL}/chat/completions`
- **`baseURL`:** configured in `opencode.json` under `provider.springtale.options.baseURL`
- **Our choice `[DECISION]`:** `http://127.0.0.1:7331/v1` — reuse the springtaled management API bind, add `/v1` prefix to match OpenAI convention. Never bind `0.0.0.0`.

### 3.2 Headers

Sent by OpenCode (via AI SDK):

| Header | Source | Example | Our use |
|---|---|---|---|
| `Authorization: Bearer <apiKey>` | AI SDK openai-compatible wrapper | `Bearer ${SPRINGTALE_TOKEN}` | validate against springtaled HMAC token |
| `Content-Type: application/json` | AI SDK | — | require |
| `Accept: text/event-stream` | AI SDK when streaming | — | require when `stream:true` |
| `x-session-affinity` | `session/llm.ts:369-384` | `<ulid>` | **formation persistence key** |
| `x-parent-session-id` | `session/llm.ts:369-384` | `<ulid>` or absent | log only |
| `User-Agent` | `session/llm.ts:369-384` | `opencode/1.4.11` | log only |
| `x-opencode-project`, `x-opencode-session`, `x-opencode-request`, `x-opencode-client` | opencode-branded providers only | — | log only |

**Critical:** OpenCode does NOT set the OpenAI `user` field in the request body. Session identity is **header-only**. Confirmed by grep against `anomalyco/opencode` — no `providerOptions.openaiCompatible.user` references.

### 3.3 Response headers we emit

```
HTTP/1.1 200 OK
Content-Type: text/event-stream
Cache-Control: no-cache
X-Accel-Buffering: no
Connection: keep-alive
```

- `text/event-stream` mandatory for streaming (AI SDK checks via EventSourceParserStream)
- `X-Accel-Buffering: no` defends against nginx/proxy buffering if users ever put one in front
- No chunked-encoding tricks — standard SSE framing only

### 3.4 SSE framing

Per `parse-json-event-stream.ts` (consumes `eventsource-parser` npm package, RFC-compliant):

- Each event: `data: <json>\n\n` (blank line terminates)
- Multi-line `data:` concatenated with `\n` (we never split; emit single-line only)
- Terminal sentinel: `data: [DONE]\n\n` is **accepted but not required** (`parse-json-event-stream.ts:24-27` — `if (data === '[DONE]') { return; }`). If we close the stream without it, the parser flushes cleanly. **Our choice `[DECISION]`:** emit `data: [DONE]\n\n` for maximum compatibility with any future non-SDK consumer.

---

## 4. Request schema — what OpenCode sends

Built in `@ai-sdk/openai-compatible` `openai-compatible-chat-language-model.ts#L231-L291`. JSON body shape:

```jsonc
{
  "model": "springtale/formation",         // REQUIRED — always set
  "messages": [ ... ],                     // REQUIRED — full history every turn
  "stream": true,                          // OpenCode always streams
  "tools": [ ... ],                        // present when agent has tools enabled
  "tool_choice": "auto",                   // "auto" | "none" | "required" | {type:"function",function:{name}}
  "max_tokens": 8192,                      // optional, from agent frontmatter or user
  "temperature": 0.1,                      // optional
  "top_p": 1.0,                            // optional
  "stop": ["\n\nHuman:"],                  // optional
  "seed": 42,                              // optional
  "response_format": {...},                // optional — we ignore
  "reasoning_effort": "medium",            // optional — we ignore
  "frequency_penalty": 0,                  // optional — we ignore
  "presence_penalty": 0                    // optional — we ignore
}
```

**Handling matrix.**

| Field | Handling |
|---|---|
| `model` | parse last path segment. `"formation"` → default formation; `"swarm"` → ≥5 members, Fever-seeking; `"solo"` → single-agent mode |
| `messages` | full history every turn — extract last user message as intent; prior turns seed blackboard priors |
| `stream` | must be `true`; reject with 400 + `{error:{message:"Springtale only supports stream:true", type:"invalid_request"}}` otherwise |
| `tools[]` | map each `{type:"function",function:{name,parameters}}` → `CapabilityDecl` for this turn. Re-project to formation each call because user may toggle tools between turns. |
| `tool_choice` | `"none"` → hard-ban tool emission; `"required"` → formation must emit ≥1 tool_call; `"auto"` → default policy; `{type:"function",function:{name}}` → constrain first emitted call |
| `max_tokens` | map to formation fuel ceiling. Output truncation triggers `finish_reason:"length"`. |
| `temperature` | 0.0 → deterministic, role transformations locked; higher → relax interference constraint, more experimental bidding in CFP |
| `stop` | ignore — not meaningful for formation output |
| `seed` | ignore — formations are deterministic-by-default already |
| everything else | ignore without erroring |

**Provider-namespaced escape hatch:** per `chat-language-model.ts:264-274`, unknown keys under `providerOptions.springtale.*` are spread into the request body. Users can inject `formation_size: 5`, `momentum_floor: "Hot"`, etc. We document these in `sdk/opencode/README.md`.

### 4.1 Message shapes (what's inside `messages`)

From `convert-to-openai-compatible-chat-messages.ts`:

```jsonc
// system
{ "role": "system", "content": "<string>" }

// user
{ "role": "user", "content": "<string>" }
// OR with attachments:
{ "role": "user", "content": [
    {"type":"text", "text":"..."},
    {"type":"image_url", "image_url":{"url":"data:image/png;base64,..."}}
  ]
}

// assistant (prior turn) with tool calls
{
  "role": "assistant",
  "content": "",   // may be empty when tool_calls present
  "tool_calls": [
    {
      "id": "call_<opaque>",
      "type": "function",
      "function": { "name": "edit", "arguments": "<stringified JSON>" }
    }
  ]
}

// tool result (from OpenCode, answering our prior tool_call)
{
  "role": "tool",
  "tool_call_id": "call_<opaque>",
  "content": "<string — stringified JSON for structured results>"
}
```

**`tool_call_id` correspondence is load-bearing.** OpenAI's spec requires every `tool_calls[i]` in an assistant message be paired with exactly one `role:"tool"` message in history (`convert-to-openai-compatible-chat-messages.ts:221-253`). If we drop a tool call mid-flight (rally/cancellation), OpenAI's validator rejects the next request. Implication: every emitted `tool_calls[i]` MUST be followed to completion, or we never emit it at all.

### 4.2 Tool definitions (what arrives in `tools[]`)

OpenCode defines tools via Zod schemas (`tool/tool.ts:119` — `Tool.define(id, ...)`). The AI SDK converts Zod → JSON Schema for the wire. Verified shapes:

```ts
// edit — tool/edit.ts:35-40
{ filePath: string, oldString: string, newString: string, replaceAll?: boolean }

// bash — tool/bash.ts:53-67
{ command: string, timeout?: number, workdir?: string, description: string }

// read — tool/read.ts:21-25
{ filePath: string, offset?: number, limit?: number }

// grep — tool/grep.ts:21-25
{ pattern: string, path?: string, include?: string }

// write — tool/write.ts:29-32
{ content: string, filePath: string }

// webfetch — tool/webfetch.ts:12-19
{ url: string, format: "text"|"markdown"|"html", timeout?: number }
```

Descriptions come from sibling `.txt` files (`edit.txt`, `bash.txt`, etc.). We **log** these on first request and commit captured schemas to `crates/springtale-opencode/tests/fixtures/opencode_tools.json` for replay tests.

---

## 5. Response schema — what we emit

### 5.1 Chunk schema (full, verbatim)

From `chat-language-model.ts:686-728`:

```ts
const chunkBaseSchema = z.looseObject({
  id: z.string().nullish(),
  created: z.number().nullish(),
  model: z.string().nullish(),
  choices: z.array(z.object({
    delta: z.object({
      role: z.enum(['assistant']).nullish(),
      content: z.string().nullish(),
      reasoning_content: z.string().nullish(),
      reasoning: z.string().nullish(),
      tool_calls: z.array(z.object({
        index: z.number().nullish(),
        id: z.string().nullish(),
        function: z.object({
          name: z.string().nullish(),
          arguments: z.string().nullish(),
        }),
      })).nullish(),
    }).nullish(),
    finish_reason: z.string().nullish(),
  })),
  usage: openaiCompatibleTokenUsageSchema,  // all fields optional
});
```

Everything is `nullish()` (optional or null). `z.looseObject` means unknown top-level fields are preserved — the SDK can pass them through as `raw` chunks when `includeRawChunks:true`.

### 5.2 `finish_reason` vocabulary

Map at `map-openai-compatible-finish-reason.ts:6-18`:

| Wire value | SDK maps to | Our use |
|---|---|---|
| `"stop"` | `stop` | intent satisfied, formation dissolves cleanly |
| `"length"` | `length` | fuel budget exhausted, token ceiling hit |
| `"tool_calls"` | `tool-calls` | emitted ≥1 tool call, waiting for results |
| `"function_call"` | `tool-calls` | legacy — we don't emit |
| `"content_filter"` | `content-filter` | consensus denied a destructive action |
| anything else | `other` | — |

### 5.3 Token usage (optional)

```jsonc
"usage": {
  "prompt_tokens": 128,
  "completion_tokens": 64,
  "total_tokens": 192,
  "prompt_tokens_details": { "cached_tokens": 32 },
  "completion_tokens_details": {
    "reasoning_tokens": 0,
    "accepted_prediction_tokens": 0,
    "rejected_prediction_tokens": 0
  }
}
```

Schema at `chat-language-model.ts:625-643` — all fields `nullish`, `looseObject`. Emitted only in the **final chunk** alongside `finish_reason`. Our choice `[DECISION]`: report `prompt_tokens` as formation fuel consumed on priors; `completion_tokens` as fuel consumed this turn. `cached_tokens` tracks blackboard-prior reuse.

---

## 6. Tool-call streaming — the hard part

From `streaming-tool-call-tracker.ts`. This is the most failure-prone area; get it wrong and OpenCode silently double-executes or drops tools.

### 6.1 Mandatory fields on the FIRST delta of each tool call

```jsonc
{
  "choices": [{
    "delta": {
      "tool_calls": [{
        "index": 0,               // REQUIRED — positional, stable across deltas
        "id": "call_<uuid>",      // REQUIRED — tracker throws InvalidResponseDataError if missing (L142-147)
        "function": {
          "name": "edit",         // REQUIRED on first delta (L149-154)
          "arguments": ""         // may start empty
        }
      }]
    }
  }]
}
```

### 6.2 Subsequent deltas

Match on `index`. Only `arguments` appended. `id` and `name` ignored on repeats (`streaming-tool-call-tracker.ts:204-212`).

```jsonc
{ "choices":[{ "delta":{ "tool_calls":[{ "index":0, "function":{ "arguments":"{\"filePath" } }] } }] }
{ "choices":[{ "delta":{ "tool_calls":[{ "index":0, "function":{ "arguments":"\":\"/auth.py\"" } }] } }] }
{ "choices":[{ "delta":{ "tool_calls":[{ "index":0, "function":{ "arguments":",\"oldString\":" } }] } }] }
// ...
```

### 6.3 CRITICAL: `arguments` fragmentation rule

Tracker re-checks `isParsableJson(...)` after every append (`streaming-tool-call-tracker.ts:188-190, 215-217`). **The first time the accumulated string parses, it finalizes the tool call immediately — even before `finish_reason:"tool_calls"` arrives.**

Two safe strategies:
1. **Atomic:** emit the entire `arguments` JSON in ONE delta chunk. Never split.
2. **Fragment carefully:** split only at points where the partial string is NOT independently valid JSON (e.g. split at `"{\"x\":"` then `"1}"` — not `{"x":1}` then `{"y":2}`).

**Our choice `[DECISION]`:** atomic. Formation members decide a complete action before we emit; there's no token-stream motivation to fragment. One delta per tool call, arguments fully formed.

### 6.4 Index fallback (avoid)

If `index` is missing, tracker falls back to `this.toolCalls.length` (L100), which means **each delta without index creates a NEW tool call.** Google Gemini's compat mode is why this exists. **Always send `index`** — we'll start at 0 and increment per distinct call.

### 6.5 Finalization

Final chunk when all tool calls are emitted:

```jsonc
{ "choices": [{ "delta": {}, "finish_reason": "tool_calls" }] }
```

Flush handler at `chat-language-model.ts:580-616` also calls `toolCallTracker.flush()` to finalize any unparseable pending calls on stream close. Belt and suspenders — we still emit `finish_reason:"tool_calls"` explicitly.

### 6.6 Parallel tool calls

Supported in schema (distinct `index` values). **Our choice `[DECISION]`:** emit serially (one at a time, wait for result, then next) in v1. Matches formation tick cadence and simplifies the state machine. Add parallel when formation cooperation spec calls for it (two members decide actions in the same tick).

### 6.7 tool_choice enforcement

`openai-compatible-prepare-tools.ts:77-90` emits one of `"auto"`, `"none"`, `"required"`, or `{type:"function",function:{name}}`. The SDK **does not validate** that our response complies. We enforce:

| Value | Formation behavior |
|---|---|
| `"auto"` | default — formation may or may not emit tool calls |
| `"none"` | formation MUST NOT emit tool calls; purely text response; if it tries, we strip before emission and log a warning |
| `"required"` | formation MUST emit ≥1 tool call; if zero after budget exhausted, return `finish_reason:"content_filter"` |
| `{type:"function",function:{name: X}}` | first emitted call MUST be `X`; constrain CFP bidding to agents with matching capability |

---

## 7. Session identity & persistence

**Header-only**, no body field (confirmed against anomalyco/opencode source).

### 7.1 Extraction

```rust
let session_key = headers
    .get("x-session-affinity")
    .and_then(|v| v.to_str().ok())
    .map(String::from)
    .unwrap_or_else(|| {
        // Fallback: deterministic hash of (user-agent, first-user-message, tools-set)
        // Keeps formations persistent when OpenCode is misconfigured or a non-OC client
        // hits the endpoint.
        deterministic_session_hash(&headers, &body.messages, &body.tools)
    });
```

### 7.2 Mapping to FormationId

`[DECISION]` One `x-session-affinity` → one `FormationId`. Persist across turns.

- On first turn: `Formation::new(...)` with `IntentPattern` derived from last user message
- On subsequent turns: look up existing formation by session_key, update intent, tick
- Formation dissolves after `N = 180s` (configurable) of idle (no incoming turn), or explicit `/spawn` with fresh intent

### 7.3 Persistence store

`[DECISION]` In-memory `DashMap<String, Arc<Formation>>` keyed by session. Survives process lifetime, not daemon restarts. Add SQLite-backed persistence later if users want resume-across-restart (needs `store::FormationRow` migration — out of scope here).

---

## 8. Slash command protocol

### 8.1 How OpenCode expands them

From `session/prompt.ts:1547-1595`:

- `$1`, `$2`, ... replaced positionally. Last-numbered placeholder absorbs remaining args joined by spaces (L1570-1576)
- `$ARGUMENTS` replaced verbatim with full args string (L1578)
- No placeholders + non-empty args: `template + "\n\n" + arguments` (L1581)
- `` !`<shell cmd>` `` executed via shell, output substituted inline (L1584-1594)
- Result `.trim()`'d, `@file` references expanded to attachments, THEN sent as a normal user message

### 8.2 Springtale command templates

Files we ship under `sdk/opencode/command/`. Each uses a **structured prefix** so the shim can reliably parse the intent.

**`sdk/opencode/command/spawn.md`:**
```markdown
---
description: Spawn a new Springtale formation with an intent
agent: springtale
---

[springtale:spawn] $ARGUMENTS
```

**`sdk/opencode/command/formation.md`:**
```markdown
---
description: Show current formation state
agent: springtale
---

[springtale:formation]
```

**`sdk/opencode/command/bots.md`:**
```markdown
---
description: List active bots and their roles
agent: springtale
---

[springtale:bots]
```

**`sdk/opencode/command/status.md`:**
```markdown
---
description: Show momentum tier, fuel budget, attention distribution
agent: springtale
---

[springtale:status]
```

### 8.3 Shim-side parsing

```rust
enum UserIntent {
    Spawn(String),         // the body after [springtale:spawn]
    ShowFormation,
    ListBots,
    ShowStatus,
    FreeForm(String),      // no springtale: prefix — treat as normal coding prompt
}

fn parse_last_user_message(msg: &str) -> UserIntent {
    let trimmed = msg.trim();
    if let Some(rest) = trimmed.strip_prefix("[springtale:spawn]") {
        UserIntent::Spawn(rest.trim().to_string())
    } else if trimmed.starts_with("[springtale:formation]") {
        UserIntent::ShowFormation
    } else if trimmed.starts_with("[springtale:bots]") {
        UserIntent::ListBots
    } else if trimmed.starts_with("[springtale:status]") {
        UserIntent::ShowStatus
    } else {
        UserIntent::FreeForm(trimmed.to_string())
    }
}
```

Classical commands (`ShowFormation`, `ListBots`, `ShowStatus`) short-circuit the formation entirely — respond with a direct SSE stream of pre-formatted text, `finish_reason:"stop"`. No fuel, no AI call.

---

## 9. Cancellation

From `post-to-api.ts:98-107` and `session/llm.ts:414-428`:

- OpenCode creates `new AbortController()` per stream
- `abortSignal` passed straight to `fetch(url, { signal })`
- Esc / Ctrl+C / app exit → `controller.abort()` → fetch abort → TCP close
- **No in-band goodbye message** is sent before abort

### 9.1 Our handler's abort detection

Axum exposes stream termination via `Drop` on the response body. We wrap the SSE stream in a guard that fires a cancellation token on drop:

```rust
pub async fn chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionRequest>,
) -> impl IntoResponse {
    let cancel = CancellationToken::new();
    let formation = state.spawn_or_resume(&req, cancel.child_token()).await;

    let stream = formation_to_sse(formation, cancel.child_token());
    let guarded = stream.map(/* ... */).chain(std::iter::once_with(move || {
        // On stream drop (client abort), fire cancellation
        cancel.cancel();
    }));

    Sse::new(guarded)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
```

### 9.2 Formation cleanup on cancel

Cancellation token propagates through formation:

1. Tick loop's `tokio::select!` picks up `cancel.cancelled()` → exits cleanly
2. Members release claimed tasks (`blackboard.release_task(id)`)
3. Active commit barriers mark aborted
4. Rally tokens released back to budget
5. OpenCode tool_calls that were emitted but not yet completed: **must not leave dangling.** We track in-flight tool_call IDs; on cancel, we DO NOT wait for results — formation state is preserved for next turn but the current tool chain is abandoned.

### 9.3 Dangling tool_calls across cancellation — the corner case

Per §4.1 every `tool_calls[i]` emitted needs a matching `role:"tool"` in future history. If OpenCode aborts mid-tool-call, the next turn's history will contain our assistant message with unresolved tool_calls — OpenAI validator rejects.

`[DECISION]` **Mitigation:** on cancellation, the formation records a synthetic tool-result internally keyed by the dangling tool_call ID. On the next incoming turn, if OpenCode replays the assistant message with those tool_calls, we inject the synthetic results into the formation's view before continuing. OpenCode's history is what it is — we make ours match.

---

## 10. Errors

### 10.1 Non-streaming errors (pre-stream)

HTTP 4xx/5xx response:

```
HTTP/1.1 401 Unauthorized
Content-Type: application/json

{
  "error": {
    "message": "Invalid SPRINGTALE_TOKEN",
    "type": "authentication_error",
    "code": "invalid_api_key"
  }
}
```

From `openai-compatible-error.ts:3-14`:

```ts
z.object({
  error: z.object({
    message: z.string(),              // REQUIRED
    type: z.string().nullish(),
    param: z.any().nullish(),
    code: z.union([z.string(), z.number()]).nullish(),
  }),
})
```

Only `error.message` is required. Degradation path at `response-handler.ts:17-81` tolerates malformed bodies (falls back to `statusText`), so we have some slack, but we emit the schema correctly anyway.

### 10.2 Streaming errors (mid-stream)

The chunk schema is `z.union([chunkBaseSchema, errorSchema])` at `chat-language-model.ts:732-736`. Emit an error chunk mid-stream:

```
data: {"error":{"message":"Formation dissolved — rally tokens exhausted","type":"formation_dissolved"}}

```

SDK's transform detects `'error' in chunk.value` at L480-487, emits `{type:'error'}` stream part, sets `finishReason='error'`, **continues reading**. So we can follow an error chunk with a final `[DONE]` for clean termination.

### 10.3 Error taxonomy

| Condition | HTTP status | `error.type` | `error.code` |
|---|---|---|---|
| Missing or invalid `Authorization` | 401 | `authentication_error` | `invalid_api_key` |
| `stream: false` | 400 | `invalid_request` | `streaming_required` |
| Unknown `model` | 400 | `invalid_request` | `model_not_found` |
| Malformed JSON body | 400 | `invalid_request` | `malformed_body` |
| Springtaled internal panic | 500 | `server_error` | `internal` |
| Formation rally exhausted mid-stream | 200 (already streaming) | emit error chunk, `type: formation_dissolved` | — |
| Fuel budget exhausted | 200 | emit `finish_reason:"length"`, no error | — |
| Consensus denies destructive action | 200 | emit `finish_reason:"content_filter"`, include context in final `content` delta | — |

### 10.4 Timeout policy

`[DECISION]` No server-side timeout. Formation ticks continue until: intent satisfied / fuel exhausted / rally exhausted / client disconnects / daemon shutdown. SSE keep-alive every 15s prevents proxy timeouts.

---

## 11. Formation event → SSE chunk mapping

The full translation table. Rows marked `[verbose]` emit only when `providerOptions.springtale.verbose == true`.

| Formation event | Suppress | SSE chunk |
|---|---|---|
| Formation spawned | always | — |
| First tick begins | never | `{"choices":[{"delta":{"role":"assistant"}}]}` |
| Member AI streams token | never | `{"choices":[{"delta":{"content":"<token>"}}]}` |
| Member classical-logic output | never | `{"choices":[{"delta":{"content":"<text>"}}]}` |
| Blackboard SubTask posted by orchestrator | [verbose] | `{"choices":[{"delta":{"content":"\n[plan] <desc>\n"}}]}` |
| Contract Net CFP announced | [verbose] | `{"choices":[{"delta":{"content":"\n[CFP] <capability>\n"}}]}` |
| Contract Net bid received | [verbose] | `{"choices":[{"delta":{"content":"  ↳ <agent> bids <utility>\n"}}]}` |
| Contract Net award | [verbose] | `{"choices":[{"delta":{"content":"  ✓ awarded to <agent>\n"}}]}` |
| Momentum tier change | [verbose] | `{"choices":[{"delta":{"content":"\n[momentum: <old> → <new>]\n"}}]}` |
| Member decides tool call | never | `{"choices":[{"delta":{"tool_calls":[{"index":N,"id":"call_<uuid>","function":{"name":"<tool>","arguments":"<full json>"}}]}}]}` |
| Tool calls complete for this turn | never | `{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}` |
| Consensus vote opened | [verbose] | `{"choices":[{"delta":{"content":"\n[vote] <question>\n"}}]}` |
| Consensus vote cast | [verbose] | `{"choices":[{"delta":{"content":"  ↳ <agent> votes <choice>\n"}}]}` |
| Consensus denies destructive action | never | `{"choices":[{"delta":{"content":"\n[denied by consensus] <reason>\n"},"finish_reason":"content_filter"}]}` |
| Interference detected | [verbose] | `{"choices":[{"delta":{"content":"\n[interference] <A> and <B>: <type>\n"}}]}` |
| Rally triggered | [verbose] | `{"choices":[{"delta":{"content":"\n[rally] <failed-agent>, <tokens-remaining> tokens\n"}}]}` |
| Rally exhausted | never | error chunk `{"error":{"message":"rally exhausted","type":"formation_dissolved"}}` + `[DONE]` |
| Member role transformation | [verbose] | `{"choices":[{"delta":{"content":"\n[role] <agent>: <old> → <new>\n"}}]}` |
| Voluntary sacrifice | [verbose] | `{"choices":[{"delta":{"content":"\n[sacrifice] <agent> → <beneficiary>\n"}}]}` |
| Fuel budget exhausted | never | `{"choices":[{"delta":{},"finish_reason":"length"}]}` + usage + `[DONE]` |
| Intent satisfied | never | `{"choices":[{"delta":{},"finish_reason":"stop"}]}` + usage + `[DONE]` |
| Cancellation (client abort) | never | stream closes; no final chunk emitted |

---

## 12. Shipped config artifacts

Dropped by `springtale opencode install`. Scope flag: `--project` (default) writes to `.opencode/` relative to cwd; `--user` writes to `~/.config/opencode/`.

### 12.1 `sdk/opencode/agent/springtale.md`

```markdown
---
description: Cooperative bot formation — decisions by bots, not one LLM
mode: primary
model: springtale/formation
tools:
  read: true
  edit: true
  bash: true
  grep: true
  write: true
  webfetch: false
permission:
  bash: ask
  edit: ask
  write: ask
  webfetch: deny
---

You are interfacing with Springtale. Each turn runs through a formation of
specialized bots cooperating via momentum tiers, Contract-Net bidding, and
consensus on destructive actions. OpenCode executes tools; Springtale decides
which tools to call.

Available commands while this agent is active:
  /spawn <intent>   create a fresh formation with the given intent
  /formation        show current formation: members, momentum, fuel
  /bots             list active bots and roles
  /status           momentum tier, fuel budget, attention distribution

Intent keywords shape formation behavior:
  reconnoiter       — explore repo, read-only
  execute           — carry out a concrete plan
  stabilize         — refactor without changing behavior
  surge             — aggressive, lowered approval threshold
```

### 12.2 `sdk/opencode/opencode.jsonc` (merge template)

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "springtale": {
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "http://127.0.0.1:7331/v1",
        "apiKey": "{env:SPRINGTALE_TOKEN}"
      },
      "models": {
        "formation": { "name": "Springtale Formation (default)" },
        "swarm":     { "name": "Springtale Swarm (≥5 bots, seeks Fever)" },
        "solo":      { "name": "Springtale Solo (single agent)" }
      }
    }
  }
}
```

### 12.3 `sdk/opencode/command/*.md` — see §8.2

### 12.4 `sdk/opencode/README.md`

Explains: prerequisite (springtaled running with opencode_bridge feature), how to set `SPRINGTALE_TOKEN`, how to verify with `curl`, how to toggle `providerOptions.springtale.verbose`.

---

## 13. Rust crate layout

Per workspace conventions: modules-over-inline, `lib.rs` is a table of contents, `thiserror` for errors, no `unwrap/expect/panic`.

```
crates/springtale-opencode/
├── Cargo.toml
└── src/
    ├── lib.rs                    # pub mod + re-exports only
    ├── error.rs                  # OpencodeBridgeError (thiserror)
    ├── router.rs                 # axum Router, mounted by springtaled
    ├── auth.rs                   # Authorization header → HMAC validation
    ├── session/
    │   ├── mod.rs
    │   ├── identity.rs           # x-session-affinity extraction + fallback hash
    │   └── store.rs              # DashMap<session_key, Arc<Formation>>
    ├── request/
    │   ├── mod.rs
    │   ├── types.rs              # ChatCompletionRequest (serde)
    │   ├── message.rs            # Message, Content, ToolCall (serde)
    │   ├── tools.rs              # ToolDef → CapabilityDecl mapping
    │   └── intent.rs             # parse_last_user_message → UserIntent
    ├── response/
    │   ├── mod.rs
    │   ├── chunk.rs              # ChatCompletionChunk emitter (serde)
    │   ├── sse.rs                # SSE framing (data: prefix, \n\n, [DONE])
    │   ├── tool_call.rs          # atomic tool_call delta emitter (§6.3)
    │   └── error.rs              # OpenAI error envelope
    ├── translate/
    │   ├── mod.rs
    │   ├── request_to_formation.rs  # OpenAI request → Formation::new / resume
    │   └── event_to_chunk.rs     # Formation event → SSE chunk (§11 table)
    ├── commands/
    │   ├── mod.rs
    │   ├── spawn.rs              # [springtale:spawn] → new formation
    │   ├── formation.rs          # [springtale:formation] → status readout
    │   ├── bots.rs               # [springtale:bots] → member list
    │   └── status.rs             # [springtale:status] → momentum/fuel/attention
    └── cancellation.rs           # CancellationToken plumbing (§9)

tests/
├── fixtures/
│   ├── opencode_tools.json       # captured from real OpenCode on first run
│   ├── request_simple.json
│   ├── request_with_tools.json
│   └── expected_sse_stream.txt
├── sse_framing.rs                # SSE wire-format unit tests
├── tool_call_streaming.rs        # §6 invariants (atomic arguments, index stability)
├── session_persistence.rs        # §7 session_key → formation reuse
├── cancellation.rs               # §9 abort → formation cleanup
└── end_to_end_replay.rs          # replay a captured OpenCode session
```

### 13.1 `lib.rs`

```rust
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod auth;
pub mod cancellation;
pub mod commands;
pub mod error;
pub mod request;
pub mod response;
pub mod router;
pub mod session;
pub mod translate;

pub use error::OpencodeBridgeError;
pub use router::build_router;
```

### 13.2 springtaled wiring

`apps/springtaled/src/server.rs` (new block, feature-gated):

```rust
#[cfg(feature = "opencode_bridge")]
{
    let oc_router = springtale_opencode::build_router(state.clone());
    app = app.nest("/v1", oc_router);
}
```

Feature on by default in `apps/springtaled/Cargo.toml`:

```toml
[features]
default = ["opencode_bridge"]
opencode_bridge = ["springtale-opencode"]
```

---

## 14. Test fixtures & replay harness

Per feedback memory rules: write deterministic tests over captured real wire traffic, not hand-rolled fakes.

### 14.1 Capture phase (one-time per OpenCode version)

Script: `scripts/capture_opencode_fixtures.sh`

1. Start `springtaled --feature=opencode_bridge --log-wire=/tmp/wire.log`
2. Configure a fresh OpenCode install with our `opencode.jsonc` and `agent/springtale.md`
3. Run OpenCode, issue canonical prompts:
   - `"hello"` (simple text)
   - `"read the Cargo.toml"` (single tool call)
   - `"fix the login bug"` (multi-tool, multi-turn)
   - `/spawn refactor auth module` (slash command)
   - `<Esc during streaming>` (cancellation)
4. `wire.log` contains request JSONs and our response SSE streams
5. Commit `wire.log` slices into `tests/fixtures/` as the replay corpus

### 14.2 Replay harness

`tests/end_to_end_replay.rs` replays each captured request against a test router built on top of a mock `Formation` that emits scripted events. Asserts byte-exact SSE output against the captured stream. Regression gate.

### 14.3 Property tests

- SSE framing: arbitrary Unicode content → parseable by `eventsource-parser` (use via node_modules in a `node --eval` sub-process or reimplement parser in Rust for tests)
- tool_call round-trip: every emitted `tool_calls[i].id` has a matching inbound `role:"tool"` in subsequent turn
- cancellation: abort at any tick boundary → formation releases all claims + rally tokens

---

## 15. Installer CLI

`apps/springtale-cli/src/commands/opencode.rs`. Subcommands:

```
springtale opencode install [--project | --user]
springtale opencode uninstall
springtale opencode doctor
```

### 15.1 `install`

1. Locate target dir: `$PWD/.opencode/` (project) or `$XDG_CONFIG_HOME/opencode/` (user)
2. Create `agent/springtale.md`, `command/spawn.md`, `command/formation.md`, `command/bots.md`, `command/status.md` from `sdk/opencode/`
3. Merge `provider.springtale` block into existing `opencode.json` (preserve other providers)
4. Prompt user to set `SPRINGTALE_TOKEN` env var (print export line for their shell)
5. Print next steps: "run `springtale opencode doctor` to verify"

### 15.2 `doctor`

1. Confirm `springtaled` is running (probe management API `/health`)
2. Confirm `opencode_bridge` feature is enabled (probe `/v1/models`)
3. Confirm `SPRINGTALE_TOKEN` is set and matches
4. Hit `/v1/chat/completions` with a canned `"hello"` request, verify streaming response completes
5. Print diagnosis with actionable remediation for each failure

### 15.3 `uninstall`

Remove the agent/command files; remove `provider.springtale` block; leave opencode.json intact.

---

## 16. Open decisions (user owns)

| # | Decision | Default proposed |
|---|---|---|
| D1 | Where does `CodeAgent` role live? | `springtale-bot/src/cooperation/role/code_agent.rs` (bot-side, can depend on springtale-ai) |
| D2 | Model names | `formation`, `swarm`, `solo` (three options in opencode.json dropdown) |
| D3 | Default session TTL | 180 seconds idle → dissolve |
| D4 | Default `verbose` surface level | `false` — keep UX clean by default |
| D5 | Bind address / port | `127.0.0.1:7331` reusing management API port |
| D6 | Ship plugin path too? | **No** — HTTP shim via `@ai-sdk/openai-compatible` covers 100%. Plugin route is under-documented and buys nothing. |
| D7 | Persistence across daemon restart? | **No** in v1. In-memory session map. Add SQLite later if requested. |
| D8 | Parallel tool calls? | **No** in v1. Serial only. |
| D9 | Fallback behavior when formation empty | Auto-spawn single-bot formation using config-default connector set |
| D10 | What do `/bots` and `/formation` output look like exactly? | Pixel-colony-themed ASCII with bot sprites + momentum meter (matches unified UI memory) |

---

## 17. References

### 17.1 Vercel AI SDK source (wire contract)

- [`@ai-sdk/openai-compatible` chat language model](https://github.com/vercel/ai/blob/main/packages/openai-compatible/src/chat/openai-compatible-chat-language-model.ts) — request builder L231-291, chunk schema L686-728, stream transform L519-616, usage schema L625-643
- [Message conversion](https://github.com/vercel/ai/blob/main/packages/openai-compatible/src/chat/convert-to-openai-compatible-chat-messages.ts) — tool result shape L221-253
- [Tool preparation](https://github.com/vercel/ai/blob/main/packages/openai-compatible/src/chat/openai-compatible-prepare-tools.ts) — tool_choice L77-90, tool schema L42-69
- [Finish reason map](https://github.com/vercel/ai/blob/main/packages/openai-compatible/src/chat/map-openai-compatible-finish-reason.ts) L6-18
- [Error schema](https://github.com/vercel/ai/blob/main/packages/openai-compatible/src/openai-compatible-error.ts) L3-14
- [Streaming tool-call tracker](https://github.com/vercel/ai/blob/main/packages/provider-utils/src/streaming-tool-call-tracker.ts) — mandatory fields L142-154, delta merging L188-217
- [SSE parser](https://github.com/vercel/ai/blob/main/packages/provider-utils/src/parse-json-event-stream.ts) — `[DONE]` handling L24-27
- [Response handler](https://github.com/vercel/ai/blob/main/packages/provider-utils/src/response-handler.ts) — error degradation L17-81
- [Fetch abort](https://github.com/vercel/ai/blob/main/packages/provider-utils/src/post-to-api.ts) L98-107

### 17.2 OpenCode source (behavior)

All paths relative to `packages/opencode/src/` on [anomalyco/opencode](https://github.com/anomalyco/opencode), `dev` branch.

- `agent/agent.ts:31, 245` — mode values + default
- `config/agent.ts:16-50, 101-136` — agent frontmatter schema + glob pattern
- `config/command.ts:17-23, 29-46` — command frontmatter + glob
- `config/config.ts:161-164` — provider record schema
- `config/provider.ts:73-109` — provider Info shape
- `provider/provider.ts:94-117` — provider adapter registry (includes `@ai-sdk/openai-compatible`)
- `session/llm.ts:5, 333, 365-384, 414-428` — AI SDK wiring, session headers, abort plumbing
- `session/prompt.ts:1547-1595` — slash command template expansion
- `tool/edit.ts:35-40`, `bash.ts:53-67`, `read.ts:21-25`, `grep.ts:21-25`, `write.ts:29-32`, `webfetch.ts:12-19` — tool parameter schemas
- `tool/tool.ts:119` — `Tool.define` macro
- `server/adapter.bun.ts` — port 4096 preferred with ephemeral fallback
- `server/routes/control/index.ts:86-88` — `/doc` OpenAPI route

### 17.3 Related Springtale docs

- `ARCHITECTURE.md` — daemon structure, management API
- `COOPERATION.md` — formation/momentum/rally/consensus primitives referenced in §11
- `COOPERATION_IMPLEMENTATION_PLAN.md` — formation crate layout
- `SECURITY.md` — HMAC auth, threat model
- `CLAUDE.md` — workspace conventions this doc follows

---

## 18. Implementation sequencing

Order to build, each phase ending with a runnable artifact.

### Phase 0 — fixture capture (1 day)
Stand up a minimal `/v1/chat/completions` that echoes requests to stdout. Run OpenCode against it. Capture request fixtures and tool schemas. Commit to `tests/fixtures/`.

### Phase 1 — skeleton + echo agent (2 days)
- `crates/springtale-opencode/` scaffolded per §13
- Router mounts under springtaled
- Auth enforced per §8
- Handler returns a hard-coded "hello from springtale" SSE stream
- `sdk/opencode/` config shipped
- `springtale opencode install` works
- **Demo:** fresh user types `@springtale` + "hi" in OpenCode, sees streaming reply

### Phase 2 — classical slash commands (2 days)
- `[springtale:formation]`, `[springtale:bots]`, `[springtale:status]` — deterministic, no formation spawn
- Pixel-colony-themed ASCII output per unified UI memory
- **Demo:** `/status` shows momentum/fuel/attention readout

### Phase 3 — formation spawn + text streaming (3 days)
- `[springtale:spawn]` creates formation via `springtale-bot::cooperation::lifecycle::spawn_formation`
- Formation tick loop → member AI adapter → SSE text deltas
- Session persistence per §7
- **Demo:** multi-turn conversation with NoopAdapter formation, momentum climbs across turns

### Phase 4 — tool call round-trip (3 days)
- Member decides tool call → atomic emission per §6.3
- OpenCode result → formation ingest
- `tool_choice` enforcement per §6.7
- **Demo:** `@springtale` asked to read a file, emits `read` tool call, OpenCode executes, formation reports file content

### Phase 5 — cancellation + errors (2 days)
- Cancellation token plumbing per §9
- Error taxonomy per §10
- **Demo:** Esc during streaming cleanly releases rally tokens

### Phase 6 — cooperation surfacing (3 days)
- Verbose mode showing CFP, consensus votes, rally events per §11
- Consensus-denies-destructive-action path with `content_filter` finish
- **Demo:** `@springtale` with 3 bots tries a destructive edit, consensus denies, reply shows vote tally

### Phase 7 — replay harness + CI (2 days)
- `tests/end_to_end_replay.rs` byte-exact against captured fixtures
- Property tests per §14.3
- CI gates on replay regression
- **Demo:** break a chunk format, replay fails loudly

**Total: ~17 days to full v1.** Each phase is merge-able independently.

---

*End of wire contract.*
