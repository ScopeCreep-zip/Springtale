# Governance: how Springtale stays accountable

If you're going to let software take actions in the real world — send
messages, change files, open pull requests — you need to be able to answer
three questions at any moment:

1. **What is it allowed to do?**
2. **Who said yes?**
3. **What did it actually do?**

Enterprise "agentic AI" frameworks in 2026 converged on a four-layer model
for answering those questions: a **Data** layer, an **Intelligence** layer,
an **Execution** layer, and a **Governance** layer that wraps all three.
Springtale was built this way from the start — not because a framework told
us to, but because the threat model demanded it. This guide maps the model
onto the actual crates so you can see where each guarantee lives.

> The short version: **reads are free, writes ask first, and everything that
> happens is written down.** The rest of this page is how.

---

## 1. The four layers

```
   ┌───────────────────────────────────────────────────────────────┐
   │  GOVERNANCE  — what's allowed, who approved, what happened     │
   │                                                               │
   │   springtale-sentinel  ·  approval gate  ·  executions log    │
   │   capability allow-list ·  ToolPolicy    ·  audit trail       │
   └───────────────┬───────────────────────────────────────────────┘
                   │ wraps every action below
   ┌───────────────▼───────────┐  ┌──────────────────────────────┐
   │  INTELLIGENCE             │  │  EXECUTION                   │
   │  (optional, swappable)    │  │  (deterministic spine)       │
   │                           │  │                              │
   │  springtale-ai adapters   │  │  springtale-runtime dispatch │
   │  3-layer AI hierarchy     │─▶│  autonomy ladder             │
   │  (unit / squad / colony)  │  │  connector execute (WASM)    │
   │  NoopAdapter by default   │  │  router + rules + cron       │
   └───────────────────────────┘  └───────────────┬──────────────┘
                                                   │ reads / writes
   ┌───────────────────────────────────────────────▼──────────────┐
   │  DATA  — state, memory, secrets, results                      │
   │                                                               │
   │  springtale-store (SQLite, STRICT tables, 0600, WAL)          │
   │  session memory · blackboard results · Secret<T> credentials  │
   └───────────────────────────────────────────────────────────────┘
```

*Fig. 1. The four layers, and the crates that implement each.*

The important property: **Governance is not a feature you can switch off.**
It's the outermost layer, and every action — whether a human typed it, a
rule fired it, or an AI proposed it — passes through it on the way to the
Execution layer. Unplug the Intelligence layer entirely (the default
`NoopAdapter`) and Governance still applies to every rule and command.

### Where each layer lives

| Layer | Crates / surfaces | Guarantees |
|---|---|---|
| **Data** | `springtale-store`, bot session memory, the cooperation blackboard | STRICT SQLite tables, `0600` file perms, WAL; credentials wrapped in `Secret<T>`; zero telemetry |
| **Intelligence** | `springtale-ai` adapters, the [3-layer AI command hierarchy](cooperation.md) | Optional and swappable; `NoopAdapter` is the default; AI *proposes*, it never holds authority |
| **Execution** | `springtale-runtime` dispatch, the router, rules, cron, connector WASM sandbox | Deterministic; every connector call is capability-checked before it runs |
| **Governance** | `springtale-sentinel`, the approval gate, the executions log, `ToolPolicy` | What's allowed, who approved, what happened — the audit spine |

---

## 2. Autonomy graduation

The industry model talks about agents "graduating" through autonomy levels
L1→L5 as they earn trust. Springtale's `AutonomyLevel` is exactly that
ladder, set **per agent** — not globally, never globally:

```
   OBSERVE ───────▶ SUGGEST ───────▶ ACT WITH ───────▶ ACT
   watch &          say what you     APPROVAL          AUTONOMOUSLY
   report only      WOULD do         claim, then       claim &
                    (don't claim)    hold for a yes    execute
   ────────────────────────────────────────────────────────────▶
   less authority                                  more authority
```

*Fig. 2. The autonomy ladder (`AutonomyLevel` in `springtale-cooperation`).*

| Level | Behavior | Industry analogue |
|---|---|---|
| `Observe` | Watches and reports. Never claims or acts. | L1 — read-only assistant |
| `Suggest` | Scans, reports what it *would* do, doesn't claim. | L2 — recommend-only |
| `ActWithApproval` | Claims work, then **blocks for a human/consensus yes** before executing. | L3/L4 — human-in-the-loop |
| `ActAutonomously` | Full autonomy — scan, claim, execute. | L5 — autonomous |

A brand-new agent defaults to `Suggest`. You move it up the ladder
deliberately, one agent at a time, as it earns your trust. There is no
"make everything autonomous" switch — that's a design refusal, not an
oversight.

---

## 3. The approval trail (human-in-the-loop)

`ActWithApproval` is where the interesting part happens. When an agent — or
a chat task you sent from your phone — wants to do something that *changes
the world*, it doesn't just do it. It pauses, durably, and asks.

This is the 2026 "durable interrupt / resume" pattern, implemented natively
(no external workflow engine):

```
   chat task / agent action
            │
            ▼
   ┌──────────────────┐   read-only?      ┌──────────────────────┐
   │  ToolPolicy /    │ ───── yes ──────▶ │  run immediately     │
   │  capability      │                   │  (no friction)       │
   │  check           │ ───── writes? ──┐ └──────────────────────┘
   └──────────────────┘                 │
                                        ▼
                         ┌─────────────────────────────┐
                         │  APPROVAL GATE              │
                         │  1. persist pending row     │  ← survives restart
                         │  2. checkpoint the loop     │  ← survives restart
                         │  3. deliver a 3-button card │  → your phone
                         └──────────────┬──────────────┘
                                        │
              ┌─────────────────────────┼─────────────────────────┐
              ▼                         ▼                         ▼
         ✅ approve                ❌ deny                  ⏱ timeout / restart
         run the BOUND            nothing runs            deny-by-default;
         action, continue,        + audit row             expired rows swept
         + audit row                                      on next boot
```

*Fig. 3. An action that changes the world pauses for approval, durably.*

Three properties make this trustworthy:

- **It survives a restart.** The pending approval and the paused task are
  written to `springtale-store` before the card is even sent. Kill the
  daemon mid-approval and an unexpired request is still waiting when it comes
  back; the task resumes against the *exact* action it paused on, not a
  re-derived one. (This follows the OWASP Agentic 2026 guidance: bind the
  approval to the specific action, with an expiry, single-use.)
- **It fails closed.** Timeout, expiry, or an unexpired request that can't be
  matched after a restart all resolve to **deny**. Silence is never a yes.
- **The card shows a summary, never raw arguments.** You approve *"post to
  #general"*, not a wall of escaped JSON you can't read.

This is the same gate whether the request came from a Telegram tap, the
in-app chat panel, or `POST /approvals/:id` on the management API — one
durable queue, three front doors.

See also: [the chat → task flow](first-bot.md) and
[the connectors guide](connectors.md) for what counts as a "write."

---

## 4. The audit trail

Every action that runs leaves a row. Not a log line that rotates away — a
queryable record in `springtale-store`:

```
   action runs ──▶ executions log ──┬─▶ what fired it (rule / chat / agent)
                                    ├─▶ what it did (connector + action)
                                    ├─▶ outcome (succeeded / failed / empty …)
                                    ├─▶ when, how long
                                    └─▶ for approvals: who said yes, when
```

*Fig. 4. The executions log — one row per action, queryable after the fact.*

Two deliberate privacy choices sit on top of this:

- **Sizes, not contents.** The executions log records that an action ran and
  whether it succeeded — it does **not** retain the message body or the
  scraped page by default. "Content not retained" is the honest default for
  users whose threat model is hostile attention.
- **It's local.** The audit trail lives in your SQLite file (`0600`, WAL),
  not a cloud you don't control. Zero telemetry. Nobody else gets to read
  what your bots did.

You can browse it in the [executions log](executions-and-drift.md) panel, or
pull it over the management API.

---

## 5. Putting it together — a worked example

You text the bot from your phone: *"delete the old logs in ~/tmp."*

```
   1. DATA          message lands, session memory updated
   2. INTELLIGENCE  AI (if attached) picks the filesystem-delete tool
                    — or a deterministic rule does; either way…
   3. GOVERNANCE    ToolPolicy: this is a WRITE → gate it
                    → pending row persisted, loop checkpointed
                    → 3-button card to your phone
   4. you tap ✅    approval resolved, attributed to you
   5. EXECUTION     the BOUND delete runs, capability-checked
   6. GOVERNANCE    executions row written: who, what, when, outcome
   7. DATA          result returned to your chat
```

Nothing happened until you said yes. What happened is written down. And if
the daemon had crashed between steps 3 and 4, the request would still have
been waiting for you — or, past its expiry, safely denied.

That's the whole point: **safety and privacy are constraints, not
features.** The governance layer is how Springtale keeps that promise even
when it's acting on your behalf, on a phone, while you're asleep.

---

*Locked design intent for the security model lives in
[`docs/current-arch/SECURITY.md`](../current-arch/SECURITY.md). This guide is
the plain-language map; that document is the contract.*
