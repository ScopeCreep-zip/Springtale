mod autolock;
mod commands;
mod paths;
mod sidecar;
mod state;

use tauri_specta::{Builder, collect_commands, collect_events};
use tracing_subscriber::EnvFilter;

/// Run the Tauri application.
///
/// Plan 2.1: the desktop app is a *client* of `springtaled`, not a second
/// copy of it. Unlocking the vault spawns one daemon as a Tauri sidecar
/// (`sidecar::start`) and the frontend talks to its HTTP API on loopback —
/// the same API and the same provider the web dashboard uses. There is
/// exactly one store, one scheduler and one bot loop, and they all live in
/// the daemon process.
///
/// The commands that remain are the ones only a desktop shell can serve:
/// the vault overlay, the window title, content protection, the tray, the
/// global quick-hide hotkey, auto-lock, and the element picker.
///
/// `mobile_entry_point` (Tauri 2): on iOS + Android the platform
/// framework loads us as a library and calls this function directly
/// — no `main` exists there. On desktop, `main.rs` still calls
/// `springtale_desktop::run()` so the same entry serves all three
/// targets. Per `v2.tauri.app/start/project-structure/`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install the post-quantum-preferring rustls crypto provider before
    // any TLS-touching code runs. See
    // `springtale_transport::crypto_provider::install_default_pq` and
    // `docs/security/CRYPTO-INVENTORY.md`. Same call lives at the head of
    // `apps/springtaled/main.rs`; both entry points need it because the
    // desktop binary is springtaled-with-a-window.
    springtale_transport::crypto_provider::install_default_pq();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Create the app shell — instant, no disk access. The vault and the
    // daemon handle are populated after the user unlocks via the frontend
    // overlay (see commands/vault.rs). This matches the
    // tauri-plugin-stronghold pattern: state exists but is empty
    // until initialize() is called with a password.
    let app_state = state::AppState::shell();

    tracing::info!("Springtale desktop starting (sidecar client)");

    // tauri-specta — single source of truth for command + event
    // signatures. The Builder's `.invoke_handler()` routes incoming
    // IPC; `.mount_events()` (called from setup) wires the typed
    // event channels; `.export()` writes `bindings.ts` so the
    // SolidJS frontend imports typed `commands.foo(...)` /
    // `events.fooBar.listen(...)` instead of hand-maintaining the
    // parameter shapes.
    //
    // ## `specta::Type` derive policy (workspace-wide)
    //
    // Only derive `Type` on a struct/enum if it appears DIRECTLY in:
    //   1. A `#[tauri::command]` parameter or return type, or
    //   2. A `#[derive(tauri_specta::Event)]` payload, or
    //   3. An explicit `Builder::typ::<T>()` registration.
    //
    // Specifically: rule-shaped types (`Rule`, `Action`, `Condition`,
    // `Trigger`) deliberately do NOT derive `Type`. They are
    // recursive (`Chain { steps: Vec<Action> }`, `And { Vec<Condition> }`,
    // etc.) and specta v2.0.0-rc.25's type-graph walker stack-overflows
    // on self-referential enums during `.export()`. Tauri commands
    // hand `Rule` payloads as `serde_json::Value` and deserialize
    // internally; the rule-builder UI reads `get_rule_schema()`'s
    // JSON Schema (schemars). This matches Spacedrive's "flat
    // projections only over IPC" pattern. Do not re-derive `Type`
    // on those types without first introducing a flat wrapper.
    let specta_builder: Builder<tauri::Wry> = Builder::<tauri::Wry>::new()
        .events(collect_events![
            commands::vault::VaultUnlocked,
            commands::vault::VaultLocked,
            commands::quick_hide::QuickHide,
        ])
        .commands(collect_commands![
            commands::vault::create_vault,
            commands::vault::unlock_vault,
            commands::vault::lock_vault,
            commands::vault::get_vault_status,
            commands::safety::set_window_title,
            commands::safety::apply_content_protection,
            commands::safety::apply_disguise_to_shell,
            commands::safety::reset_auto_lock,
            commands::tray::apply_disguise_to_tray,
            commands::quick_hide::apply_quick_hide_shortcut,
            commands::selector_picker::open_selector_picker,
        ]);

    // .export() is intentionally NOT called yet.
    //
    // specta v2.0.0-rc.25's built-in `Type` impl for
    // `serde_json::Value` is broken — see specta-rs/specta#455 and
    // PR #454. The implementation in `legacy_impls.rs:90` declares
    // `serde_json::Value` as `inline`, then its variant for `Array`
    // references `Vec<Value>` (which re-expands `Value` inline), so
    // `.export()` infinite-recurses through Value's own definition
    // and stack-overflows even on a 32MB stack. Every command and
    // type that carries arbitrary connector configs, AI configs,
    // recipe inputs, etc. transits via `serde_json::Value`, so we
    // can't avoid the bug.
    //
    // PR #454 (merged 2026-03-10) is supposed to fix this by
    // emitting `Value` as a named data type instead of inlining,
    // but rc.25 (the latest published as of 2026-05-11) still
    // ships the broken legacy impl. Keep the `Type` derives + the
    // tauri-specta Builder in place so we get typed IPC routing,
    // typed event channels (no Value involved), and re-enable
    // `.export()` once a fixed specta release lands. Frontend
    // continues to use the hand-maintained `dashboard/types.ts`
    // interfaces in the interim.
    //
    // To re-enable when specta ships the fix:
    //
    // ```
    // #[cfg(debug_assertions)]
    // {
    //     use specta_typescript::Typescript;
    //     specta_builder
    //         .export(Typescript::default(), "../src/bindings.ts")
    //         .expect("failed to export tauri-specta bindings.ts");
    // }
    // ```

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .manage(commands::quick_hide::ActiveQuickHide::default())
        .invoke_handler(specta_builder.invoke_handler())
        .setup(move |app| {
            specta_builder.mount_events(app);
            // G5f tray-icon disguise: build a single tray handle at
            // startup so the safety chain can swap its icon + tooltip
            // at runtime. Failure to build (rare; some Linux WMs) is
            // logged and the rest of the app continues — graceful
            // degradation matters in a coercive setting where any
            // crash is worse than a missing disguise channel.
            match commands::tray::init(app) {
                Ok(_handle) => {
                    tracing::info!("tray icon initialised");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "tray icon init failed; disguise still applies window-title + content protection");
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running springtale desktop");
}
