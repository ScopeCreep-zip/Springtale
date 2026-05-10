mod autolock;
mod commands;
pub mod runtime_guard;
mod state;

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

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::connectors::list_connectors,
            commands::connectors::list_available_connectors,
            commands::connectors::setup_connector,
            commands::connectors::enable_connector,
            commands::connectors::disable_connector,
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
            commands::vault::create_vault,
            commands::vault::unlock_vault,
            commands::vault::lock_vault,
            commands::vault::get_vault_status,
            commands::safety::get_safety_config,
            commands::safety::save_safety_config,
            commands::safety::set_window_title,
            commands::panic::panic_wipe,
            commands::travel::travel_prepare,
            commands::travel::travel_restore,
            commands::canvas::get_connections,
            commands::canvas::get_canvas_state,
            commands::canvas::update_canvas,
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
            // Config
            commands::config::get_config,
            commands::config::set_config,
            commands::config::list_config,
            commands::config::set_ai_adapter,
            commands::config::set_connector_config,
            commands::config::configure_ai_adapter,
            commands::config::upsert_connector_config,
            commands::config::toggle_formation_guard,
            // Agent
            commands::agent::list_agent_states,
            commands::agent::get_autonomy,
            commands::agent::set_autonomy,
            commands::agent::step_autonomy,
            // Authors
            commands::authors::list_authors,
            commands::authors::add_author,
            commands::authors::remove_author,
            // Data
            commands::data::export_data,
            // Memory
            commands::memory::audit_memory,
            commands::memory::compact_memory,
            // Diagnostics
            commands::diagnostics::run_diagnostics,
            // Onboarding
            commands::onboarding::list_onboarding_platforms,
            commands::onboarding::apply_onboarding,
            // Templates
            commands::templates::list_templates,
            commands::templates::write_template,
            // Fixes
            commands::fixes::list_fixes,
            commands::fixes::get_fix,
            commands::fixes::apply_fix,
            // Send
            commands::send::send_message,
            // Bot
            commands::bot::bot_status,
            commands::bot::bot_memory,
            // Sessions
            commands::sessions::list_sessions,
            // Heartbeat
            commands::heartbeat::get_heartbeat,
            commands::heartbeat::set_heartbeat,
        ])
        .run(tauri::generate_context!())
        .expect("error while running springtale desktop");
}
