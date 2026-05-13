//! Stealth patches — injected via `Page::execute_on_new_document` to
//! avoid incidental headless detection on sites with reflexive
//! blocks (RSS scrapers, own-dashboard automation).
//!
//! ## Policy boundaries
//!
//! Per `feedback_no_ban_risk`: Springtale never markets anti-bot
//! bypass as a feature. The default [`crate::config::StealthProfile`]
//! is `Off`. Opt-in is per-connector-config; surfaced in the recipe
//! authoring UX as "incidental detection avoidance," not "bypass
//! Cloudflare." Use cases this is intended for:
//!   - Polling RSS feeds that happen to be behind a Cloudflare WAF
//!     with overzealous bot filtering.
//!   - Authoring own-dashboard automation against tools that detect
//!     `navigator.webdriver` reflexively (e.g. some SaaS admin
//!     panels).
//! Use cases this is **not** intended for:
//!   - Bypassing a site's explicit anti-scraping policy.
//!   - Evading rate limits or fraud detection.
//!
//! ## Patch set ("Minimal" profile — three high-signal evasions)
//!
//! Per [DataDome 2026 threat research][dd] + [Playwright stealth
//! retrospective][stealth-2026], these three move the needle in 2026
//! and don't carry their own detection signatures:
//!
//! 1. **`navigator.webdriver = undefined`** (NOT `false`). DataDome
//!    et al. specifically test for the patched value `false` as a
//!    tell-tale of stealth tooling. Deleting the property is the
//!    correct restoration.
//! 2. **HeadlessChrome UA stripping**. Pair the launch flag
//!    `--disable-blink-features=AutomationControlled` (applied in
//!    `client::launch`) with this script: re-define `navigator.userAgent`
//!    via the prototype so any `HeadlessChrome` substring is
//!    replaced. Belt + braces because some detectors check
//!    `userAgent` from a Worker context where the launch flag
//!    doesn't propagate.
//! 3. **`window.chrome` object completion**. A bare `window.chrome`
//!    object missing `loadTimes()` / `csi()` is itself a fingerprint
//!    — real Chrome ships both methods. We restore them with
//!    realistic shapes (no real timing data leaks — the methods
//!    return stable stub values).
//!
//! ## Explicitly omitted patches (don't ship these)
//!
//! - **`iframe.contentWindow` proxy** — DataDome detects the
//!   evasion's internal code AND it crashes some sites' DOM
//!   ([berstend/puppeteer-extra#909][bugref]).
//! - **`navigator.plugins` faking** — fake `PluginArray` objects
//!   fail `instanceof PluginArray` checks. Doing it badly is worse
//!   than not doing it.
//! - **WebGL vendor spoofing** — only useful with a consistent
//!   OS/hardware story; spoofing GPU alone creates a fresh
//!   fingerprint.
//! - **Battery API / Media codecs / WebRTC IP-leak patches** —
//!   battery API is deprecated; codecs / WebRTC need network-layer
//!   fixes, not JS patches.
//!
//! [dd]: https://datadome.co/threat-research/how-datadome-detects-puppeteer-extra-stealth/
//! [stealth-2026]: https://dev.to/vhub_systems_ed5641f65d59/playwright-stealth-mode-in-2026-the-7-patches-that-actually-matter-46bp
//! [bugref]: https://github.com/berstend/puppeteer-extra/issues/909

pub mod patches;

pub use patches::{minimal_patch_script, MINIMAL_LAUNCH_FLAGS};
