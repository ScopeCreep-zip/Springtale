mod autolock;
mod commands;
pub mod runtime_guard;
mod state;

use tauri_specta::{collect_commands, collect_events, Builder};
use tracing_subscriber::EnvFilter;

/// Run the Tauri application.
///
/// The desktop app IS springtaled with a GUI. Same runtime underneath:
/// store, rule engine, connector registry, AI adapter, sentinel.
/// Tauri adds the window. springtaled adds the HTTP API.
///
/// Per ARCHITECTURE.md §9:
/// 1. SolidJS frontend (no business logic, no secrets)
/// 2. Tauri IPC bridge (typed commands + events)
/// 3. Commands layer (validates inputs, delegates to crates)
/// 4. Core crates (pure Rust, zero Tauri dependency)
///
/// `mobile_entry_point` (Tauri 2): on iOS + Android the platform
/// framework loads us as a library and calls this function directly
/// — no `main` exists there. On desktop, `main.rs` still calls
/// `springtale_desktop::run()` so the same entry serves all three
/// targets. Per `v2.tauri.app/start/project-structure/`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Create the app shell — instant, no DB access. The runtime is
    // populated after the user unlocks the vault via the frontend
    // overlay (see commands/vault.rs). This matches the
    // tauri-plugin-stronghold pattern: state exists but is empty
    // until initialize() is called with a password.
    let app_state = state::AppState::shell();

    tracing::info!("Springtale desktop starting (deferred runtime)");

    // W1.F — the approval dispatcher lives next to AppState so the
    // dispatcher task (spawned at vault unlock) and the
    // `respond_to_approval` command share the keyed-oneshot map.
    let approval_dispatcher = app_state.approval_dispatcher.clone();

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
            commands::approval::ApprovalRequired,
            commands::vault::VaultUnlocked,
            commands::vault::VaultLocked,
            commands::quick_hide::QuickHide,
            commands::workspaces::ChatDiscovered,
        ])
        .commands(collect_commands![
            commands::connectors::list_connectors,
            commands::connectors::list_available_connectors,
            commands::connectors::setup_connector,
            commands::connectors::enable_connector,
            commands::connectors::disable_connector,
            commands::connectors::reload_connector,
            commands::connectors::get_connector_schemas,
            commands::connectors::remove_connector,
            commands::connectors::remove_connector_cascade,
            commands::connectors::get_connector_config,
            commands::connectors::list_connector_outputs,
            commands::connectors::install_connector,
            commands::rules::get_rule_schema,
            commands::rules::list_rules,
            commands::rules::create_rule,
            commands::rules::toggle_rule,
            commands::rules::delete_rule,
            commands::rules::update_rule,
            commands::rules::run_rule,
            commands::rules::parse_rule,
            commands::rules::create_connector_rule,
            commands::rules::list_rules_for_connector,
            commands::rules::test_connector,
            commands::rules::reassign_rule_connector,
            commands::events::list_events,
            commands::executions::list_executions,
            commands::executions::get_execution_steps,
            commands::executions::vacuum_executions,
            commands::drift::get_recipe_drift,
            commands::drift::get_rule_drift,
            commands::workspaces::list_workspaces,
            commands::workspaces::scan_workspaces,
            commands::workspaces::delete_workspace,
            commands::workspaces::upsert_workspace_manual_cmd,
            commands::workspaces::preview_onboard_url,
            commands::workspaces::start_onboard_stream,
            commands::workspaces::cancel_onboard_stream,
            commands::test_step::test_recipe_step,
            commands::selector_picker::open_selector_picker,
            commands::vault::create_vault,
            commands::vault::unlock_vault,
            commands::vault::lock_vault,
            commands::vault::get_vault_status,
            commands::safety::get_safety_config,
            commands::safety::save_safety_config,
            commands::safety::set_disguise_active,
            commands::safety::set_disguise_profile,
            commands::safety::set_panic_tap_count,
            commands::safety::set_window_title,
            commands::safety::apply_disguise_to_shell,
            commands::safety::apply_content_protection,
            commands::tray::apply_disguise_to_tray,
            commands::quick_hide::apply_quick_hide_shortcut,
            commands::panic::panic_wipe,
            commands::travel::travel_prepare,
            commands::travel::travel_restore,
            commands::canvas::get_connections,
            commands::canvas::get_canvas_state,
            commands::canvas::subscribe_canvas,
            commands::cooperation::subscribe_cooperation,
            commands::safety::reset_auto_lock,
            commands::formations::create_formation,
            commands::formations::deploy_formation,
            commands::formations::pause_formation,
            commands::formations::resume_formation,
            commands::formations::dissolve_formation,
            commands::formations::rally_formation,
            commands::formations::list_formations,
            commands::formations::get_formation,
            commands::formations::update_formation_intent,
            commands::formations::add_formation_member,
            commands::formations::remove_formation_member,
            commands::formations::list_intents,
            commands::formations::deploy_team,
            commands::formations::cycle_formation_intent,
            commands::formations::cycle_formation_autonomy,
            commands::formations::formation_commands,
            commands::formations::formation_eligible_members,
            commands::config::get_config,
            commands::config::set_config,
            commands::config::list_config,
            commands::config::set_ai_adapter,
            commands::config::set_connector_config,
            commands::config::configure_ai_adapter,
            commands::config::upsert_connector_config,
            commands::config::toggle_formation_guard,
            commands::agent::list_agent_states,
            commands::agent::get_autonomy,
            commands::agent::set_autonomy,
            commands::agent::step_autonomy,
            commands::authors::list_authors,
            commands::authors::add_author,
            commands::authors::remove_author,
            commands::data::export_data,
            commands::memory::audit_memory,
            commands::memory::compact_memory,
            commands::diagnostics::run_diagnostics,
            commands::onboarding::list_onboarding_platforms,
            commands::onboarding::apply_onboarding,
            commands::templates::list_templates,
            commands::templates::write_template,
            commands::fixes::list_fixes,
            commands::fixes::get_fix,
            commands::fixes::apply_fix,
            commands::send::send_message,
            commands::bot::bot_status,
            commands::bot::bot_memory,
            commands::sessions::list_sessions,
            commands::heartbeat::get_heartbeat,
            commands::heartbeat::set_heartbeat,
            commands::recipes::list_recipes,
            commands::recipes::get_recipe,
            commands::recipes::list_recipe_categories,
            commands::recipes::toggle_recipe_favorite,
            commands::recipes::record_recipe_recent,
            commands::recipes::apply_recipe,
            commands::recipes::render_recipe_toml,
            commands::recipes::preflight_recipe,
            commands::recipes::preview_recipe,
            commands::recipes::list_recipe_pieces,
            commands::recipes::save_user_recipe,
            commands::recipes::fork_recipe,
            commands::recipes::delete_user_recipe,
            commands::recipes::export_recipe_toml,
            commands::recipes::import_recipe_toml,
            commands::approval::respond_to_approval,
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
        .manage(app_state)
        .manage(approval_dispatcher)
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
