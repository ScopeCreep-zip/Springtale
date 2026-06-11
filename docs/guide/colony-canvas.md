# Colony Canvas

The colony canvas is the primary UI for Springtale. It's an RTS-style live
view of your running connectors, rules, agents, and formations. Same
component tree runs in two places:

- **Tauri desktop** (`tauri/apps/desktop`) — talks to `springtaled` through
  IPC commands that call directly into `springtale-runtime`.
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

Springtails move when their rules fire. Nodes pulse on incoming events.
Mycelium thickens when throughput is high.

## 2. The three-pane layout

```
  ┌───────────────────────────────────────────────────────────┐
  │  TopBar                                                   │
  │  status · formation switcher · settings · panic           │
  ├──────────────┬────────────────────────────────────────────┤
  │              │                                            │
  │ ConnectorBar │    Viewport (ColonyCanvas)                 │
  │              │                                            │
  │ nodes list   │    • pan / zoom                            │
  │ install ctl  │    • click to select                       │
  │              │    • drag to reposition                    │
  │              │    • 1-9 select agents                     │
  │              │    • Esc deselects                         │
  │              │                                            │
  ├──────────────┴────────────────────────────────────────────┤
  │  BottomPanel (selection-context command grid)             │
  │                                                           │
  │  [ selection-specific commands — see §3 ]                 │
  └───────────────────────────────────────────────────────────┘
```

## 3. The command grid

The 3×3 command grid at the bottom changes with selection context
(StarCraft-style). Every button does something real — no empty slots.

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

Buttons map to `/formations/*` endpoints, which in turn push a
`FormationCommand::{Deploy, Pause, Resume, ChangeIntent, AddMember,
RemoveMember, Rally, Dissolve}` onto the bot's command channel.

**Agent selected:** Agent-level commands — Autonomy cycle (observe →
suggest → approve → autonomous), view health, step through decisions.

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

## 6. Simlish bubbles

Agents emit Simlish-style short speech bubbles. Bubbles map to real
activity — they are **not** random decoration. A `!` bubble means the
agent fired a rule; a `~` means waiting; a `!!` means error. Emission is
driven by `TickReport::action_taken` and the supervisor's health/liveness
outputs.

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
