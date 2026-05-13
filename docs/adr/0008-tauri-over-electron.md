# ADR 0008: Tauri 2 for the desktop shell, not Electron

**Status:** Accepted
**Date:** 2026-03-28

## Context

Springtale ships a desktop app — the "colony canvas" RTS-style
formation visualisation, plus settings, vault unlock, panic UI, and
the safety controls. The shell choice shapes:

- Binary size (matters when shipping to users on metered connections).
- Memory footprint (matters when users run this on older devices).
- Security posture (the desktop process has direct access to the vault).
- Native API surface (system tray, global hotkeys, OS keychain).
- Build complexity (matters for contributors).

## Decision

Use [Tauri 2](https://tauri.app) with a SolidJS frontend. Rust backend
in `tauri/apps/desktop/src-tauri/`, frontend in
`tauri/apps/desktop/src/`. Shared UI components live in
`tauri/packages/ui/`.

## Consequences

Positive:

- Binary size: ~15 MB per platform. Electron equivalents are
  100–200 MB.
- Memory footprint: ~150 MB. Electron equivalents are 300–500 MB
  per window.
- Tauri's backend is Rust — same language, same `cargo`, same
  toolchain as the daemon. Contributors don't need to learn Node
  for the desktop's backend.
- Tauri 2's IPC is typed via commands. The frontend can't call into
  the backend without an explicit `tauri::command` entry point.
  Better security boundary than Electron's nodeIntegration model.
- System tray, global hotkeys, file dialogs, OS keychain — all
  first-class.
- Uses the system's WebView (WebKit on macOS, WebView2 on Windows,
  WebKitGTK on Linux). No Chromium bundled. Updates ride the OS.
- ts-rs generates TypeScript types from the Rust IPC layer
  (`tauri/packages/types/src/generated/`). Hand-maintained TS types
  go away.

Negative:

- System WebView means the rendering surface varies across OSes.
  Subtle CSS differences. We have a small set of workarounds in
  `colony.css`.
- WebKitGTK on Linux is the laggard — some 2024 web platform features
  aren't there. The dashboard frontend deliberately doesn't depend on
  bleeding-edge web APIs.
- Tauri 2 was a v2 reset of the v1 API. Migration was non-trivial.
  We landed on v2 directly to avoid two migrations.
- Auto-updater isn't yet wired. We'll add it before 1.0; Tauri's
  built-in updater verifies signed releases.

Locks in:

- The frontend's IPC layer is the typed `invoke()` style — every
  command is a Rust function. No raw IPC channels.
- The backend's command registry is the security boundary. Anything
  not in the registry can't be invoked from the frontend, full stop.
- SolidJS, not React/Vue/Svelte — see ADR-future on SolidJS choice.

## Alternatives considered

### Option A — Tauri 2 (picked)

Pros and cons enumerated above.

### Option B — Electron

Pros: largest ecosystem, mature, knowable.
Cons: 100–200 MB binary. Chromium bundled (separate auto-update,
separate CVE surface). Node.js in the main process by default —
contributors need to manage three runtimes (Node, browser, our Rust
daemon). Security model is "we promise we won't enable
nodeIntegration".

Why we didn't pick it: binary size + the Chromium-as-attack-surface
concern. Our threat model includes "this runs on a user's device";
shipping another Chromium adds another patching obligation we don't
want.

### Option C — Native: GTK / Qt / Cocoa

Pros: smallest binaries. Closest to native UX.
Cons: three frontends. Three sets of bugs. The colony canvas is
graphics-heavy (RTS-style visualisation) and we'd be reimplementing
that in three places.

Why we didn't pick it: development cost dwarfs the binary-size win.

### Option D — Slint

Pros: pure Rust UI framework, small binaries, growing.
Cons: less mature than Tauri. Smaller community. The canvas
visualisation would need to be rebuilt against Slint's primitives.

Why we didn't pick it: it'd be a great choice for a greenfield UI,
but we have a working SolidJS dashboard that the web SPA also uses.
Tauri lets us share that frontend across desktop + web.

### Option E — Headless daemon + web dashboard only

Pros: simplest. One frontend.
Cons: desktop users want a native-feeling app. Tray icon, global
hotkeys, file dialog for backup paths — all of these are messy in a
browser tab. Plus the safety features (quick-hide, disguise tray)
fundamentally require native APIs.

Why we didn't pick it: the target user wants both. We ship both — the
dashboard runs on web, the same UI runs in Tauri for the native
experience.

## References

- `tauri/apps/desktop/src-tauri/` — Tauri backend
- `tauri/apps/desktop/src/` — frontend
- `tauri/packages/ui/` — shared SolidJS components
- `tauri/packages/types/src/generated/` — ts-rs output
- [Tauri 2 docs](https://tauri.app)
- Related: ADR 0007 (Axum) — both backends serve the same UI; the
  web dashboard talks to springtaled, the desktop talks to its own
  Tauri backend
