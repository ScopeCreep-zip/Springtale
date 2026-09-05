# Tutorial 03: LLM research swarm

This is the tutorial where Springtale's cooperation framework earns
its keep. You'll build a 3-agent formation:

- **Researcher** — pulls source material via the Presearch connector.
- **Writer** — composes a writeup from the research.
- **Critic** — reads the writeup and posts a critique back to the
  blackboard.

The three agents share a blackboard. The writer reads what the
researcher posted. The critic reads what the writer produced. They
all run on the same formation tick, coordinated by cadence, escalating
through momentum tiers as they build coherence.

**Time:** ~60 minutes. This is genuinely a different shape from the
first two tutorials.

**You'll need:**

- Springtale installed.
- An AI adapter wired. Cheapest path: install
  [Ollama](https://ollama.com), pull a small model (`llama3.1:8b`),
  and point Springtale at it. Production path: Anthropic / OpenAI
  API key.
- A Presearch API key (free tier OK — research load is light).

## Step 1 — Wire the AI adapter

Local-first option (Ollama):

```bash
# Install Ollama, pull a model:
ollama pull llama3.1:8b

# Tell Springtale to use it for the whole colony:
springtale-cli config ai set --scope colony --type ollama \
  --base-url http://127.0.0.1:11434 --model llama3.1:8b
```

Or API option (Anthropic):

```bash
# --api-key-stdin reads the key from stdin — never argv, never env, never TOML
springtale-cli config ai set --scope colony --type anthropic \
  --model claude-sonnet-4-6 --api-key-stdin
```

AI adapters are per level, not global: `--scope formation <id>` or
`--scope agent <rule-id>` gives one formation or one agent its own
adapter, and the dashboard exposes the same settings.

Verify:

```bash
springtale-cli config ai get --scope colony   # shows which adapter is configured
```

## Step 2 — Set up the Presearch connector

Paste the API key into the Presearch connector's card in the dashboard
(or the connector setup API). API key from
[presearch.com](https://presearch.com).

Verify:

```bash
springtale-cli connector list | grep presearch
```

Should show `connector-presearch` as Active.

## Step 3 — Scaffold the formation

Use the LLM swarm template:

```bash
springtale-cli new llm-swarm
# Creating llm-swarm project in ~/.local/share/springtale/projects/llm-swarm-20260610-091500
cd ~/.local/share/springtale/projects/llm-swarm-*/
```

You now have a working starter. Look at what shipped:

```bash
cat springtale.toml
ls rules/
```

The template sets up three agents (researcher, writer, critic) but
points them at a placeholder topic. We'll wire it to take real
research requests next.

## Step 4 — Deploy the formation

```bash
springtale-cli formation deploy-team --from rules/research-swarm.toml
```

Behind the scenes this:

1. Creates a `Formation` with intent `"research this topic"`.
2. Adds three agents as members, each tied to the configured AI
   adapter.
3. Marks the formation `active` so the cadence bus starts ticking it.

Inspect:

```bash
springtale-cli formation list
springtale-cli formation get research-swarm
```

The formation starts at **Cold momentum** — its members can read the
shared environment but not yet write to it. As they successfully
cooperate, momentum will climb. See
[`docs/guide/cooperation.md`](../guide/cooperation.md) §3 for the
full momentum mechanic.

## Step 5 — Submit a research request

The template ships with an HTTP endpoint that accepts research
requests and posts them to the formation's blackboard:

```bash
curl -X POST http://127.0.0.1:8080/formations/research-swarm/inputs \
  -H "Authorization: Bearer $(springtale-cli auth print)" \
  -H "Content-Type: application/json" \
  -d '{
    "topic": "how Wasmtime epoch deadlines work",
    "constraints": ["cite official docs", "max 500 words"]
  }'
```

What happens in the next ~30 seconds:

1. **Tick N**: The researcher agent (via the `scan` step of its
   per-agent loop) sees the new `task:research` key on the
   blackboard. It claims it (consensus + commit barrier hold while
   it works).
2. **Tick N+1 through N+3**: Researcher calls `connector-presearch`
   to search the topic + scrape top results. Posts findings as
   `task:research:complete` on the blackboard.
3. **Tick N+4**: The writer's `scan` step finds the research output,
   composes a writeup using the AI adapter, posts as
   `task:writeup:complete`.
4. **Tick N+5**: The critic's `scan` step finds the writeup, runs it
   through the AI adapter with a critique prompt, posts critique as
   `task:critique:complete`.

Between each step, the formation's momentum tier may advance:

- **Warming** after 3 successful ticks — the blackboard becomes
  writable; agents can post intermediate state.
- **Hot** after 8 — consensus voting unlocks (the critic can veto a
  writeup before it gets published).
- **Fever** after 15 — the orchestrator unlocks; AI-driven intent
  decomposition becomes available for multi-step research.

## Step 6 — Watch the formation tick

In another terminal:

```bash
springtale-cli trace --formation research-swarm
```

You'll see real-time output of each agent's 5-step loop (sense, scan,
react, respond_cfp, inbox) and the 14-step formation tick.

The colony canvas at `http://127.0.0.1:8080/dashboard` shows the
formation as a dashed zone with three members. Click the zone to see
detail: momentum, rally tokens, attention load distribution.

## Step 7 — Get the output

The critique-complete event is the signal that work is done:

```bash
curl -X GET "http://127.0.0.1:8080/formations/research-swarm/results" \
  -H "Authorization: Bearer $(springtale-cli auth print)" | jq .
```

You'll get the research, the writeup, and the critique as three
fields. The writeup is what you'd publish; the critique is what to
review before publishing.

## Step 8 — Watch a failure → rally

Try an unanswerable topic on purpose:

```bash
curl -X POST http://127.0.0.1:8080/formations/research-swarm/inputs \
  -H "Authorization: Bearer $(springtale-cli auth print)" \
  -H "Content-Type: application/json" \
  -d '{"topic": "the secret URL of the moon"}'
```

The researcher will fail (no useful search results). Watch what
happens:

1. Researcher reports failure.
2. Tick processor logs interference.
3. Supervisor's per-member check fires.
4. Cascade detector decides: is this a single failure or a pattern?
5. Single failure → rally. A rally token is burned. Attention
   redirects toward the failing agent. Next tick tries again.
6. After 3 rally token burns with no success, the formation escalates
   to the orchestrator (if at Fever tier) or dissolves (if not).

You'll see this in `trace` output. See
[`docs/guide/cooperation.md`](../guide/cooperation.md) §4 for the
rally + recovery + supervision model.

## Step 9 — Dissolve

```bash
springtale-cli formation dissolve research-swarm
```

What happens:

1. All three agents stop accepting new work.
2. One final tick runs to persist the formation's mental model to
   the `mental_model_*` SQLite tables.
3. The formation row is marked `dissolved`.

The mental model survives. If you re-deploy a formation with the
same `formation_id`, it warm-starts from what the previous instance
learned (vocabulary, successful patterns, observed conventions). See
[`docs/guide/mental-model.md`](../guide/mental-model.md).

## What you just learned

| Concept | Where it appeared |
|---|---|
| AI adapter wiring | Step 1 |
| Formation deployment from template | Step 4 |
| Blackboard as the coordination medium | Step 5 — `task:*` keys |
| Per-agent 5-step loop | Step 6 — `trace` output |
| 14-step formation tick | Step 6 — `trace` output |
| Momentum tier progression | Step 5 — Cold → Warming → Hot → Fever |
| Rally / recovery / supervision | Step 8 — failure path |
| Mental model persistence across dissolves | Step 9 |

## Extend this

| Add | See |
|---|---|
| Cross-formation gossip (two swarms watching each other) | [guide/cross-formation.md](../guide/cross-formation.md) |
| Custom roles (translator, code-reviewer, etc.) | [guide/cooperation.md §11](../guide/cooperation.md) |
| Pacing (don't burn API tokens at full speed) | [guide/pacing.md](../guide/pacing.md) |
| Intervention (orchestrator changes intent mid-run) | [guide/intervention.md](../guide/intervention.md) |
| Tool calling (researcher invokes other connectors via AI) | [cookbook: LLM with tools](../cookbook/llm-with-tools.md) |
| Consensus voting (critic vetoes bad writeups) | [guide/consensus.md](../guide/consensus.md) |

## Safety notes

- **The researcher has `NetworkOutbound` capability** for Presearch's
  domain. It can't reach arbitrary hosts. Capabilities are
  per-connector, declared in manifest, enforced at dispatch.
- **AI adapter calls bypass capability for prompt content** but tool
  calls go through the same capability gate. An LLM asking the
  researcher to "delete all rules" hits the sentinel before it hits
  the rule engine.
- **Mental model contents are sensitive.** If your formation
  processes data the formation users would not want stored, set the
  formation's `[cooperation] persist_mental_model = false` to keep
  it ephemeral.

## Cleanup

```bash
springtale-cli formation dissolve research-swarm
springtale-cli rule delete --formation research-swarm
springtale-cli connector remove connector-presearch
# AI adapter stays — useful for other formations
```
