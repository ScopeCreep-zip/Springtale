# Visual gallery

What Springtale looks like in motion. This page is meant to be
filled with screenshots and short GIFs — placeholders below until we
ship the assets.

If you're a contributor with the desktop running, dropping fresh
screenshots into `docs/logo/screenshots/` and linking them here is
a welcome PR. See "Capturing screenshots" at the end.

## The colony canvas

The desktop app's main view. An RTS-style ecosystem with
connectors as trees, rules as springtails, formations as dashed
zones, pipelines as mycelium lines.

```
+--------------------------------------------------------------+
|                                                              |
|       /\        ~~~~~~~                                      |
|      /  \   ___/       \___                                  |
|     /    \-/                \                                |
|    [GitHub]   ──mycelium──   [Telegram]                      |
|         \                       /                            |
|          \  *  ~  !  ──── springtail agents                  |
|           \---formation:research-squad----                   |
|            *  *  *   Warming   3/3                           |
|                                                              |
|       [Bluesky]      [Discord]      [Kick]                   |
|                                                              |
+--------------------------------------------------------------+
```

*Placeholder for: docs/logo/screenshots/canvas-overview.png*

Each tree's size encodes how busy the connector is. Springtail
opacity encodes health (Alive / Suspect / Down). Formation zones
have a momentum-colored border (Cold gray → Warming yellow → Hot
orange → Fever red).

## Formation detail card

Clicking a formation zone opens a card with:

```
+------------------------------------------------------------+
|  research-squad                              [GUARD on]    |
|                                                            |
|  Intent: Reconnoiter                                       |
|  Momentum: Hot (8 successes, 0 interference)               |
|  Rally tokens: ●●○  (2 of 3 remaining)                     |
|                                                            |
|  Attention                                                 |
|  ▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░░░                          |
|  researcher: 50%   writer: 30%   critic: 20%               |
|                                                            |
|  Members                                                   |
|  •  researcher        ✓ Alive    Healthy                   |
|  •  writer            ✓ Alive    Healthy                   |
|  •  critic            ~ Suspect  Healthy                   |
|                                                            |
|  Commands                                                  |
|  [PAUSE] [RESUME] [RALLY] [INTENT] [DISSOLVE]              |
|  [ADD MBR] [RM MBR] [DEPLOY]                               |
+------------------------------------------------------------+
```

*Placeholder for: docs/logo/screenshots/formation-detail.png*

The command grid is status-aware. PAUSE is disabled if the formation
is already paused; DISSOLVE is disabled if the guard toggle is on.

## Safety panel

Settings for disguise, quick-hide, panic-tap count, duress vault
setup.

```
+------------------------------------------------------------+
|  Safety                                                    |
|                                                            |
|  Disguise tray icon                                        |
|  ( ) Springtale (default)                                  |
|  (•) Calculator                                            |
|  ( ) Files                                                 |
|  ( ) Notes                                                 |
|  [ Apply ]                                                 |
|                                                            |
|  Quick-hide shortcut                                       |
|  Current: Ctrl + Shift + H                                 |
|  [ Change… ]                                               |
|                                                            |
|  Panic tap count                                           |
|  Number of taps before wipe: [ 3 ]                         |
|  [ Apply ]                                                 |
|                                                            |
|  Duress passphrase                                         |
|  Status: configured (set 2026-04-12)                       |
|  [ Test ] [ Re-set ]                                       |
|                                                            |
|  [⚠ PANIC NOW] ────  triple-tap to confirm                 |
+------------------------------------------------------------+
```

*Placeholder for: docs/logo/screenshots/safety-panel.png*

## Rule builder overlay

The visual rule builder (basic version shipped; full version with
drag-and-drop is in progress).

```
+------------------------------------------------------------+
|  Build a rule                                              |
|                                                            |
|  [Trigger ▼]   ConnectorEvent on connector-kick            |
|                event: stream_live                          |
|                                                            |
|  [Conditions]                                              |
|  + FieldEquals  trigger.broadcaster.username == "me"       |
|  + (And/Or)                                                |
|                                                            |
|  [Actions]                                                 |
|  + connector-bluesky::create_post                          |
|       text: "${trigger.title}"                             |
|  + connector-discord::send_message                         |
|       channel_id: "1234..."                                |
|       text: "${trigger.title}"                             |
|                                                            |
|  [ Preview TOML ]  [ Test run ]  [ Save & enable ]         |
+------------------------------------------------------------+
```

*Placeholder for: docs/logo/screenshots/rule-builder.png*

## Event ribbon

Live stream of trigger fires, action dispatches, sentinel verdicts.
Sits along the bottom of the canvas. Each event is a colored chip
that fades over ~10 seconds.

```
[•trigger]  [✓dispatch]  [▲sentinel:Go]  [✓dispatch]  [⚠rally]  [✓dispatch]
       └─── most recent on the right                                 │
                                                  oldest fading ─────┘
```

*Placeholder for: docs/logo/screenshots/event-ribbon.gif*

Click any chip to pin it to a side panel showing the full payload.

## Mental model inspector

For a formation, what it has learned. Shown in the BottomPanel
when you've selected a formation with `momentum >= Warming`.

```
+------------------------------------------------------------+
|  Mental model (research-squad)                             |
|                                                            |
|  Domain knowledge (12 entries)                             |
|    "github webhook for repo X arrives within 2 seconds"    |
|    "presearch returns ~3 useful results for security qs"   |
|    "telegram channel <id> is the announce target"          |
|    [ ... 9 more ... ]                                      |
|                                                            |
|  Cooperation patterns (4 entries)                          |
|    "researcher confidence > 0.7 → writer succeeds"         |
|    "critic veto rate ~15% — within tolerance"              |
|    [ ... ]                                                 |
|                                                            |
|  Vocabulary                                                |
|    "incident" → escalated work item                        |
|    "writeup"  → composed output                            |
|                                                            |
|  Persisted at last dissolve: 2026-04-30 14:21              |
+------------------------------------------------------------+
```

*Placeholder for: docs/logo/screenshots/mental-model.png*

## Cross-formation gossip

When you have multiple active formations, the gossip overlay shows
their views of each other.

*Placeholder for: docs/logo/screenshots/cross-formation.gif*

## Onboarding flow

First-run experience: vault creation, passphrase, optional connector
pairing.

*Placeholder for: docs/logo/screenshots/onboarding-1-passphrase.png*
*Placeholder for: docs/logo/screenshots/onboarding-2-templates.png*
*Placeholder for: docs/logo/screenshots/onboarding-3-canvas.png*

---

## Capturing screenshots

If you want to contribute screenshots:

1. Run the daemon + desktop in a clean state (`SPRINGTALE_DATA_DIR=
   /tmp/clean-springtale`).
2. Deploy the `telegram-bot-echo` or `llm-swarm` recipe to populate
   the canvas with real-looking data.
3. **Use placeholder values** for connector tokens. Never include
   real tokens or real chat IDs in screenshots.
4. **Redact identifying values** before posting — channel IDs,
   usernames, bot tokens, anything tied to your real identity.
5. Save as PNG (lossless). Crop to relevant region.
6. Strip EXIF: `exiftool -all= screenshot.png`.
7. Drop in `docs/logo/screenshots/` with a descriptive filename.
8. Update this page to reference the new asset and remove the
   placeholder.

For GIFs, [peek](https://github.com/phw/peek) (Linux) or
[Kap](https://getkap.co) (macOS) work well. Keep them under 5 MB
each — clicking around quickly is more useful than long takes.
