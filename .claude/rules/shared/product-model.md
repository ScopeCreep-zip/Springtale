# Springtale Product Model

## What this is

Springtale is a local-first, privacy-preserving bot automation platform.
It replaces OpenClaw (250K+ stars, riddled with CVEs) with something safe,
free, and open source. Built for people whose safety depends on privacy.

## The bot-first model

**Bots are the primary unit of interaction.** Not connectors, not rules, not AI.

A bot is:
- A command router (deterministic, classical — `/search`, `/remind`, `/status`)
- A set of connectors it can use (Telegram, GitHub, Kick, etc.)
- Rules that fire automatically (cron, webhook, filesystem watch)
- An AI adapter (optional, per-bot — defaults to NoopAdapter)
- Session memory (per-user, per-channel, SQLite-backed)
- A persona (name, identity, behavior config)

**AI is a socket, not the foundation.** The entire platform works without AI.
Users plug in Ollama, OpenAI, or Anthropic per-bot. When AI hype dies,
unplug the adapter. Nothing breaks. Every command, rule, and automation
continues to work.

## How settings are scoped

Settings are NOT global. They're layered:

```
App settings (vault, safety, language, data export)
  └── Formation settings (intent, members, momentum)
       └── Bot/Agent settings (connectors, rules, AI adapter, autonomy level)
```

- **App settings** = things about THIS instance of Springtale
  - Vault lock/unlock, passphrase
  - Safety config (auto-lock, window title, content protection)
  - Language/locale
  - Data export/import
  - Panic wipe

- **Formation settings** = things about a GROUP of agents working together
  - Intent (reconnoiter, execute, stabilize, surge)
  - Member roster (which agents belong)
  - Momentum tier (cold/warm/hot/fever — determines capabilities)

- **Bot/Agent settings** = things about a SINGLE automation
  - Which connector it lives on (its "tree")
  - What rule triggers it
  - AI adapter (none, ollama, openai, anthropic)
  - Autonomy level (observe → suggest → approve → autonomous)

## The colony UI

The desktop app and web dashboard render a **pixel-art ecosystem visualization**
inspired by RTS games (StarCraft, RimWorld) and colony sims (ONI, Factorio).

```
Connectors → Trees (pixel sprites positioned on canvas)
Rules/Bots → Springtail agents (pixel sprites near their tree)
Pipelines  → Mycelium lines (SVG paths between trees)
Swarms     → Formation zones (dashed ellipses with momentum labels)
```

Reference design: `docs/intended-arch/springtale-colony-v8.html`
Cooperation framework: `docs/intended-arch/COOPERATION.pdf`

### Visual communication

Agents communicate their state through:
- **Simlish bubbles** — short words that map to real activity (not random)
- **Activity CSS classes** — firing, error, waiting, active, idle
- **Fuel/HP bars** — only visible when degraded
- **Status symbols** — `*` active, `!` firing, `!!` error, `~` waiting, `-` idle

### Game-inspired interaction

- Click to select trees/agents/formations
- Drag trees to reposition on canvas
- Command grid (3x3, StarCraft-style) changes per selection context
- Keyboard shortcuts (1-9 select agents, Escape deselects)
- Confirm dialogs for destructive actions (detach, dissolve, remove)

## What not to do

- Don't design global AI toggles. AI is per-bot.
- Don't put connector config in app settings. Config is per-connector.
- Don't treat rules as the primary UI element. Bots/agents are.
- Don't add decorative fake data. Every visual signal maps to real state.
- Don't make settings that tell users to "edit a TOML file." If it needs
  configuring, build the UI for it.
- Don't leave empty command buttons. Every button does something real.
