# Colony Canvas

The colony canvas is the primary UI for Springtale. It's an RTS-style live
view of your running connectors, rules, agents, and formations. Same
component tree runs in two places:

- **Tauri desktop** (`tauri/apps/desktop`) — a sidecar client. `springtaled` is
  spawned on unlock and owns all state; the shell talks to it over HTTP on
  loopback through the same provider the dashboard uses. Tauri IPC is reserved
  for what the OS owns (vault unlock, tray, quick hide, content protection).
- **Web dashboard** (`tauri/apps/dashboard`) — SPA served by `springtaled`,
  talks through HTTP + SSE.

Both surfaces share `tauri/packages/ui`. A `DataProvider` abstraction
hides the transport difference from the components.

## 1. The glyph vocabulary

Every visual signal maps to real backend state. No decorative fake data.

```
  Connectors → nodes       (pixel sprites positioned on the canvas)
  Rules/Agents → springtails (pixel sprites anchored near their node)
  Formations → zones       (dashed ellipses with momentum labels)
  Pipelines → mycelium     (SVG paths between nodes)
```

Springtails move toward what they are doing. `getAgentPosition`
(`tauri/packages/ui/src/colony/geometry.ts`) places an agent one of three
ways: an agent with no connector sits on a deterministic seeded floor
spread; an agent whose activity is `firing` **and** which names an action
connector is interpolated 35 % of the way along the mycelium toward that
target node (`WALK_FRACTION`); otherwise it parks near its home node at a
standoff that widens with its attention load. Movement is a CSS transition
(`left` / `top`, 0.8 s), and a position change of more than 0.5 % adds an
`is-walking` walk-cycle class for 850 ms. Nodes pulse on incoming events.
Mycelium thickens when throughput is high.

Activity is not polled from a status field — it is the newest unexpired
**utterance** (`activityOf`, `dashboard/activity.ts`), whose vocabulary is a
closed Rust enum (`UtteranceKind` in
`crates/springtale-cooperation/src/utterance/types.rs`: Firing, Working,
Listening, Idle, Failed, Down, Claimed, Yield, Helping, Rally, Cascade).
That string becomes the sprite's CSS class directly. When every utterance
has expired the agent falls back to `listening`. There is no `Math.random()`
anywhere in the frontend data path.

## 2. The three-pane layout

```
  ┌───────────────────────────────────────────────────────────┐
  │  TopBar                                                   │
  │  status · formation switcher · settings · panic           │
  ├──────────────┬────────────────────────────────────────────┤
  │              │                                            │
  │ ConnectorBar │    Viewport (ColonyCanvas)                 │
  │              │                                            │
  │ nodes list   │    • wheel = zoom (cursor-anchored)        │
  │ install ctl  │    • middle-drag = pan                     │
  │              │    • left-click select, left-drag moves    │
  │              │      a node                                │
  │              │    • 1-9 select agents                     │
  │              │    • O cycles the overlay                  │
  │              │    • Esc deselects                         │
  ├──────────────┴────────────────────────────────────────────┤
  │  BottomPanel (selection-context command grid)             │
  │                                                           │
  │  [ selection-specific commands — see §3 ]                 │
  └───────────────────────────────────────────────────────────┘
```

The viewport is a transform wrapper, not a scroll container: `Viewport.tsx`
applies `translate(x, y) scale(z)` with `transform-origin: 0 0`. Wheel zoom
is anchored on the cursor and clamped to 0.5×–3× in 1.1× steps. Panning is
**middle-button drag only** — left-drag is reserved for repositioning nodes.
There is no reset-to-1× control; the only programmatic camera move is
`centerOn`, which the alert stack uses when you click an entry to jump to
its subject, and it preserves the current zoom.

Pressing `O` cycles the canvas overlay through `none` → `momentum` →
`attention` → `fuel`. An overlay recolours sprites by tinting them with
`--colony-overlay`; a sprite with no reading for that overlay is dimmed
(opacity 0.25, desaturated) rather than given an invented colour.

The top-right event feed shows the last five raw events. The **event
ribbon** below it is an alert stack, not a toast queue: every entry is
derived from live state on each render, so it disappears the moment its
condition stops holding, and no entry is on a wall-clock timer. It raises
five conditions — pending approvals (with inline APPROVE / DENY),
cascade hits, sentinel quarantines, members marked down (cleared by a later
recovery action for the same agent), and unexpired `failed` utterances —
deduplicated newest-per-subject, sorted error → warn → ok, capped at eight.
Dismissing an entry is forgotten once its condition ends, so a recurrence
alerts again.

## 3. The command grid

The command grid at the bottom is a fixed 3×3 and changes with selection
context (StarCraft-style). All nine slots are always laid out; a slot with
no command for the current context renders as an empty cell
(`<div aria-hidden="true" />`), never as an empty button, so a verb never
moves between contexts. Bottom-right is the only destructive cell and always
goes through a confirm dialog. A repo lint
(`tauri/scripts/check-command-verbs.mjs`) fails the build if a declared verb
has no handler.

**No selection:** App-level commands — Settings, Safety, Vault, Data
export, Panic wipe.

**Connector node selected:** Connector-level commands — Configure, Enable,
Disable, Test, Remove.

**Formation zone selected:**

```
  ┌─────────┬─────────┬─────────┐
  │ DEPLOY  │ PAUSE   │ RESUME  │
  ├─────────┼─────────┼─────────┤
  │ RALLY   │ INTENT  │ GUARD   │
  ├─────────┼─────────┼─────────┤
  │ ADD MBR │ RM MBR  │ REMOVE  │
  └─────────┴─────────┴─────────┘
```

The formation grid is the one context that is not a static table: the
backend sends `CommandDecl[]` (`crates/springtale-runtime/src/operations/commands.rs`)
carrying each verb's `enabled` flag and `disabled_reason`, and the grid
renders what it is given. Buttons map to `/formations/*` endpoints, which in
turn push a `FormationCommand::{Deploy, Pause, Resume, ChangeIntent,
AddMember, RemoveMember, Rally, Dissolve}` onto the bot's command channel.

**Agent selected:** Agent-level commands — all nine slots filled. Autonomy
cycles through four levels: `Observe`, `Suggest`, `ActWithApproval`,
`ActAutonomously` (`AutonomyLevel` in `crates/springtale-core/src/policy.rs`),
labelled OBSERVE / SUGGEST / APPROVE / AUTONOMOUS in the UI and rendered as
four pips.

## 4. Formation detail card

Selecting a formation opens a detail card in the bottom panel with:

- **Intent** — the pattern string (editable via INTENT button)
- **Momentum tier** — Cold / Warming / Hot / Fever
- **Status** — draft / active / paused (encoded via `data-status` CSS)
- **Rally pips** — remaining rally tokens, rendered Monster Hunter-style
  (filled pip = available, dim pip = spent)
- **Guard status badge** — whether guard mode is engaged
- **Aggregate stats row** — operational / load / fuel
- **Attention distribution bar** — zero-sum aggro meter (Army of Two),
  each member gets a slice proportional to their attention load
- **Member roster** — one line per member with health icon, liveness
  indicator, role name, and the connectors they own

## 5. Agent detail card

Selecting an agent opens an agent-level card with:

- **Health badge** — Healthy / Degraded / Incapacitated / Dead
- **Liveness indicator** — Alive / Suspect / Down, encoded via opacity
- **LOAD stat bar** — attention load 0..1
- **Autonomy level** — current stance
- **Fuel remaining** — FuelAmount consumed vs. initial
- **Role** — current `DynamicRoleTrait` name (General / Information /
  Support / custom transformed role)
- **Active task** — what's in `active_task`, if any
- **Recent reports** — tick reports scrolling in from the cadence bus

## 6. Motes

Agents speak in motes: a glyph from the shipped `Springtale Symbols` font
(a subset of Symbols Nerd Font Mono built by `scripts/build-symbol-font.sh`)
inside an ISO 3864 shape — triangle for warning, circle for prohibition,
square for information, colour fixed to shape. Every mote is a real
`utterance` event from the cooperation ring (`GET /cooperation/utterances`
is the def table); nothing is random. A mote expires on the colony tick
clock (`seq + ttl_ticks > now`), not on a wall-clock timer, so a paused
daemon leaves its motes where they were. At most three show per agent,
newest on top; directional glyphs mirror under RTL; every slot carries an
`aria-label` from the `utter.*` keys in each locale dictionary.

**Comprehension check before freezing a glyph.** ISO 9186's method, scaled
to the community: show each glyph-plus-shape without its label to at least
ten readers per shipped locale (`en`, `es`, `pt`, `fr`, `ar`, `th`, `tl`,
`ja`), ask what it means, keep it if 85 percent answer within the intended
meaning, otherwise override it in that locale's `locales` map in
`crates/springtale-cooperation/src/utterance/defs.rs` and rerun the font
script. Results are tracked here:

| Glyph (kind) | Locale | Readers | Within meaning | Decision |
| --- | --- | --- | --- | --- |
| _none yet_ | | | | |

## 7. Attention economy (visual)

The attention bar on the formation detail card is a live render of
`AttentionEconomy`. Sum across members is always 1.0 (zero-sum). When a
member's load exceeds a threshold, a warning dot appears on their
springtail in the viewport.

## 8. Theming

Two themes ship:

- **Colony (forest)** — original theme, pixel forest palette, Silkscreen
  pixel font
- **Chiral diorama** — Death-Stranding-inspired theme; default since
  April 2026

Theme is a CSS variable set; no backend behaviour changes when the theme
switches. Colors come from the soil palette in `colony.css`, not Tailwind
defaults. Sprite classes are defined in `@layer components`, not inline
`box-shadow`.

## 9. Confirm dialogs

Destructive actions (REMOVE formation, detach connector, dissolve, panic
wipe, vault duress setup) show a confirm dialog. Panic wipe shows a
countdown. No single misclick can destroy state.

## 9.1. In-app chat dock

You can talk to your bot without wiring up any external chat platform.
The chat dock (`ChatDock` / `ChatPanel` in `tauri/packages/ui/src/colony/`)
opens a conversation with the bot over the synthetic `in-app` connector —
the same command router, session memory, and conversational task-setup
engine that Telegram or Discord messages flow through, so "send me the
weather in Tucson every morning" deploys a recipe from here too.

Pending `ShellExec` approvals raised by in-app tasks render inline in the
chat panel (backed by `GET /approvals` + `POST /approvals/{id}`), so you
approve or deny without leaving the conversation. On the web dashboard
the same panel runs over `POST /chat` + the `/chat/stream` SSE feed; the
desktop app uses the `chat` Tauri command.

## 10. Data flow

```
  springtaled                     LiveFormationReader
      │                                    │
      │ /canvas/stream (SSE)               │ enriched formation state
      │ /formations/*  (HTTP)              │ (momentum, rally, attention,
      │ /events/stream (SSE)               │  guard, health, liveness)
      ▼                                    ▼
  ┌─────────────────────────────────────────────┐
  │  DataProvider (desktop: Tauri IPC, web: HTTP)│
  └─────────────────────┬───────────────────────┘
                        ▼
              createDashboardState(provider)
                        │
                        ▼
                   useDashboard()
                        │
                        ▼
            ColonyCanvas / BottomPanel components
```

Component code never calls `invoke()` or `fetch()` directly — it goes
through the provider. That keeps the component tree transport-agnostic.

## References

- [1] Cooperation tick that produces the state shown here: [cooperation.md](cooperation.md)
- [2] Component source: `tauri/packages/ui/src/colony/`
- [3] Desktop IPC commands: `tauri/apps/desktop/src-tauri/src/commands/`
- [4] `LiveFormationReader` trait: `crates/springtale-runtime/src/`
- [5] Reference visual: `docs/intended-arch/springtale-colony-v8.html`
