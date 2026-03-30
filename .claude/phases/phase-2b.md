# Phase 2b — Desktop + Mobile + Safety

> Source: `docs/current-arch/ARCHITECTURE.md` §3, §9, §2.6-2.8, §16
> Depends on: Phase 2a complete

## Goal

Tauri 2 desktop and mobile shell. Visual rule builder. Web dashboard.
Browser automation. Safety features for vulnerable users. Accessibility.

## What Ships

- Tauri 2 app — macOS, Windows, Linux, iOS, Android (one codebase)
- `springtale-dashboard` — web control UI served by springtaled
- `connector-browser` — headless Chromium, sandboxed
- Visual rule builder (generates TOML)
- Canvas/A2UI — live UI surface for bot content
- Safety: duress passphrase, panic wipe, travel mode, app disguise, quick-hide
- Accessibility: screen readers, i18n, RTL, keyboard nav, high contrast

## Tauri Shell Architecture

Four-layer stack (mirrors Rekindle):

```
SolidJS + TypeScript Frontend
  Rendering, state display, user input
  No business logic. No secrets. No crypto.
  ipc/ module: typed invoke() wrappers only
──────────────────────────────────────────
Tauri 2 IPC Bridge
  Commands: Frontend -> Rust (user actions)
  Events: Rust -> Frontend (state updates)
  Window management, system tray, plugins
──────────────────────────────────────────
Tauri Commands (src-tauri/commands/)
  Thin layer. Validates inputs (garde). Delegates to crates.
──────────────────────────────────────────
Core Crates (same as springtaled)
  All pure Rust. Zero Tauri dependency. Independently testable.
```

**How to build:**
- `tauri/src-tauri/` — Rust side. Tauri commands wrap springtale crate calls.
- `tauri/src/` — SolidJS frontend. Components, stores, IPC wrappers, styles.
- SolidJS is officially supported by `create-tauri-app`. Use the `solid-ts` template.
- IPC: typed `invoke()` wrappers in `ipc/` module. No raw `tauri.invoke` strings.
- State in SolidJS stores (`createStore`). No localStorage, no sessionStorage.
- Tailwind 4 utility classes. No inline `style=` props. No `@apply` except in `styles/`.

**Research needed:** Tauri 2 plugin APIs (dialog, notification, clipboard, biometric).
`tauri_plugin_dialog` for native modals (not `tauri::api::dialog` — that's Tauri 1).
SolidJS i18n via `@solid-primitives/i18n`. Tauri 2 mobile plugin development
(Swift for iOS, Kotlin for Android).

## Safety Features (§2.6-2.8)

**Duress passphrase:**
- Two Argon2id-derived keys from two passphrases
- Vault file contains two encrypted regions (real + decoy), padded to constant size
- Duress passphrase unlocks decoy profile (minimal config, no chat history)
- Duress unlock logs to hidden audit trail (accessible only with real passphrase)

**Panic wipe:**
- Tauri: configurable gesture (e.g., 5-tap on status bar)
- Mobile: shake + volume button combo
- Zeroes vault key material, overwrites vault file, clears SQLite, optionally uninstalls
- Must complete within 3 seconds

**Travel mode:**
- `springtale travel prepare --backup-to <path>` — encrypted backup + local wipe
- `springtale travel restore --from <path>` — restore from backup
- QR code restore option for mobile

**App disguise (mobile):**
- Generic app icon and name (configurable: "Notes", "Calculator", etc.)
- No app switcher content (Tauri `secureWindow` flag)
- No lock screen widgets
- Quick-hide gesture: minimize to last-used app

**Research needed:** Tauri 2 `secureWindow` API for app switcher blanking.
iOS app icon change at runtime (possible via alternate icons API).
Android dynamic launcher alias for icon/name change.

## Visual Rule Builder

SolidJS component that generates TOML rule files.

**How to build:**
- Enumerates available triggers from installed connector manifests
- Enumerates conditions from the `Condition` enum
- Enumerates actions from connector `ActionDecl` schemas
- Drag-and-drop or form-based composition
- Output: same TOML format as hand-authored rules
- "Test Rule" button: dry-run evaluation against sample data
- "Save as TOML" button: write to rules directory
- No lock-in — visual builder produces the same files the CLI creates

## Dashboard

`springtale-dashboard` — lightweight SPA served by springtaled.

**How to build:**
- Shares SolidJS component library with Tauri app
- Served on management API port (default 127.0.0.1:8081)
- HMAC bearer token auth (same as management API)
- Features: connector status, rule management, event log viewer, heartbeat config
- No cookies (HMAC token in Authorization header). No CSRF risk.
- SSE endpoint for real-time event log streaming

## Accessibility (§16)

- WAI-ARIA on all interactive elements
- Full keyboard navigation with visible focus indicators
- CSS custom properties for all colors — system high-contrast query respected
- `prefers-reduced-motion` — no animations by default
- All text in rem units — respects system font size
- RTL support via CSS logical properties
- Priority languages: English, Spanish, Portuguese, French, Arabic, Thai, Tagalog, Japanese

## Mobile

- Tauri 2 targets Android 7+ / iOS 13+
- Swift (iOS) + Kotlin (Android) plugins for: camera (QR scanning), biometric auth, push notifications, voice input, NFC
- Device pairing via QR code or Bonjour/mDNS discovery
- On-device AI: llama.cpp via Tauri native plugin (desktop/flagship phones only, not older devices)
- Home server AI: pair with springtaled → route AI calls to user's Ollama over LAN/VPN

## connector-browser

Headless Chromium via `headless_chrome` Rust crate (Chrome DevTools Protocol).

- Domain allow-list in manifest: `Capability::NetworkOutbound { host }` per domain
- `BrowserNavigate`, `BrowserFormFill`, `BrowserScreenshot` capabilities
- Cannot navigate to unapproved sites
- Actions: `navigate`, `fill_form`, `screenshot`, `extract_text`, `click`

## Not In Phase 2b

- No Veilid transport (Phase 3)
- No Rekindle bot bridge (Phase 3)
- No distributed connector registry (Phase 3)
- No P2P encrypted AI chat (Phase 3)
