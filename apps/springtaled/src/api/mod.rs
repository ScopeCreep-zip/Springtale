pub mod agents;
pub mod approvals;
pub mod auth;
pub mod authors;
pub mod bot;
pub mod canvas;
pub mod chat;
pub mod config_api;
pub mod connectors;
pub mod dashboard;
pub mod data;
pub mod diagnostics;
pub mod drift;
pub mod events;
pub mod executions;
pub mod extractors;
pub mod fixes;
pub mod formations;
pub mod health;
pub mod memory;
pub mod onboarding;
pub mod recipes;
pub mod rules;
pub mod safety;
pub mod send;
pub mod sessions;
pub mod state;
pub mod stream;
pub mod templates;
pub mod utterances;
pub mod webhooks;
pub mod workspaces;

/// Maximum length for API path parameters. Prevents DoS via oversized route strings.
const MAX_PATH_SEGMENT_LEN: usize = 256;

/// Body limit for WASM connector uploads — the sandbox memory ceiling is
/// 64 MiB, so nothing larger could run anyway.
const WASM_UPLOAD_LIMIT: usize = 64 * 1024 * 1024;

/// Validate that a path parameter is within acceptable length.
pub fn validate_path_param(param: &str) -> Result<(), axum::http::StatusCode> {
    if param.len() > MAX_PATH_SEGMENT_LEN {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    Ok(())
}

use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{StatusCode, header};
use axum::middleware;
use axum::routing::{delete, get, post, put};
use tower::ServiceBuilder;
use tower::buffer::BufferLayer;
use tower::limit::RateLimitLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use state::AppState;

/// Build the complete axum router for the management API.
///
/// Route groups:
/// - Public: /health, /ready (no auth required)
/// - Authenticated: everything else (requires Bearer token)
///
/// Middleware (applied to all routes):
/// - Rate limiting: configurable requests per second (default 100)
/// - Request body limit: 1 MiB
/// - HTTP tracing via tower-http
pub fn build_router(state: AppState) -> Router {
    let rate_limit = state.rate_limit_per_sec;

    // Public routes — no authentication
    let public = Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready));

    // Authenticated routes — require Bearer token
    let authenticated = Router::new()
        .route("/connectors", get(connectors::list))
        .route("/connectors/schemas", get(connectors::schemas))
        .route("/connectors/available", get(connectors::list_available))
        .route("/connectors/setup", post(connectors::setup))
        .route("/connectors/install", post(connectors::install))
        .route("/connectors/{name}", delete(connectors::remove))
        .route(
            "/connectors/{name}/cascade",
            delete(connectors::remove_cascade),
        )
        .route("/connectors/{name}/config", get(connectors::get_config))
        .route("/connectors/{name}/outputs", get(connectors::list_outputs))
        .route("/connectors/{name}/enable", post(connectors::enable))
        .route("/connectors/{name}/disable", post(connectors::disable))
        .route("/connectors/{name}/reload", post(connectors::reload))
        .route("/connectors/{name}/test", post(rules::test_connector))
        .route(
            "/connectors/{name}/upsert-config",
            post(config_api::upsert_connector_config),
        )
        .route("/rules", get(rules::list).post(rules::create))
        .route("/rules/parse", post(rules::parse))
        .route("/rules/schema", get(rules::schema))
        .route("/rules/{id}", put(rules::update).delete(rules::delete))
        .route("/rules/{id}/toggle", post(rules::toggle))
        .route("/rules/{id}/run", post(rules::run))
        .route("/rules/{id}/reassign", post(rules::reassign))
        .route("/rules/connector", post(rules::create_connector_rule))
        .route("/rules/connector/{name}", get(rules::list_for_connector))
        .route("/events", get(events::list))
        // Executions log + drift (plan 2.5 web parity)
        .route("/executions", get(executions::list))
        .route("/executions/vacuum", post(executions::vacuum))
        .route("/executions/{id}/steps", get(executions::steps))
        .route("/drift/recipe/{id}", get(drift::recipe))
        .route("/drift/rule/{id}", get(drift::rule))
        // External-workspace directory (plan 2.5 web parity)
        .route(
            "/workspaces",
            get(workspaces::list)
                .post(workspaces::upsert_manual)
                .delete(workspaces::delete),
        )
        .route("/workspaces/scan", post(workspaces::scan))
        .route("/workspaces/onboard-url", post(workspaces::onboard_url))
        .route("/sessions", get(sessions::list))
        .route(
            "/config/heartbeat",
            get(config_api::get_heartbeat).put(config_api::set_heartbeat),
        )
        .route("/canvas", get(canvas::get_canvas))
        .route("/canvas/connections", get(canvas::get_connections))
        .route("/webhook/{connector}/{trigger}", post(webhooks::receive))
        .route("/send", post(send::send))
        .route("/diagnostics", get(diagnostics::list))
        .route("/onboarding/platforms", get(onboarding::list))
        .route("/onboarding/{platform}", post(onboarding::apply))
        .route("/templates", get(templates::list))
        .route("/templates/{name}", post(templates::write))
        .route("/fixes", get(fixes::list))
        .route("/fixes/{id}", get(fixes::get))
        .route("/fixes/{id}/apply", post(fixes::apply))
        .route(
            "/formations",
            get(formations::list).post(formations::create),
        )
        .route("/formations/{id}", get(formations::get))
        .route("/formations/{id}/commands", get(formations::commands))
        .route(
            "/formations/{id}/run-command",
            post(formations::run_command),
        )
        .route(
            "/formations/{id}/members/eligible",
            get(formations::eligible_members),
        )
        .route("/formations/{id}/deploy", post(formations::deploy))
        .route("/formations/{id}/pause", post(formations::pause))
        .route("/formations/{id}/resume", post(formations::resume))
        .route("/formations/{id}/dissolve", post(formations::dissolve))
        .route("/formations/{id}/rally", post(formations::rally))
        .route("/formations/{id}/intent", put(formations::update_intent))
        .route(
            "/formations/{id}/propose-intent",
            post(formations::propose_intent),
        )
        .route(
            "/formations/{id}/votes/{vote_id}",
            post(formations::cast_vote),
        )
        .route(
            "/formations/{id}/members",
            post(formations::add_member).delete(formations::remove_member),
        )
        .route("/formations/intents", get(formations::list_intents))
        .route("/formations/deploy-team", post(formations::deploy_team))
        .route(
            "/formations/{id}/cycle-intent",
            post(formations::cycle_intent),
        )
        .route(
            "/formations/{id}/cycle-autonomy",
            post(formations::cycle_autonomy),
        )
        .route(
            "/formations/{id}/toggle-guard",
            post(config_api::toggle_formation_guard),
        )
        .route("/safety", get(safety::get_config).put(safety::save_config))
        // G5d — focused endpoints for the IPV duress surface so a
        // single tab flip doesn't have to round-trip the whole config.
        .route("/safety/disguise/active", post(safety::set_disguise_active))
        .route(
            "/safety/disguise/profile",
            post(safety::set_disguise_profile),
        )
        .route("/safety/panic_tap_count", post(safety::set_panic_tap_count))
        .route("/safety/panic-wipe", post(safety::panic_wipe))
        .route("/travel/prepare", post(safety::travel_prepare))
        .route("/travel/restore", post(safety::travel_restore))
        // W1.B — Recipes (click-and-play library)
        .route("/recipes", get(recipes::list))
        .route("/recipes/categories", get(recipes::list_categories))
        .route("/recipes/{id}", get(recipes::get_one))
        .route("/recipes/{id}/favorite", post(recipes::toggle_favorite))
        .route("/recipes/{id}/recent", post(recipes::record_recent))
        .route("/recipes/{id}/apply", post(recipes::apply))
        .route("/recipes/{id}/render", post(recipes::render))
        .route("/recipes/{id}/preflight", post(recipes::preflight))
        .route("/recipes/{id}/preview", post(recipes::preview))
        .route("/recipes/{id}/pieces", get(recipes::list_pieces))
        .route("/recipes/{id}/test-step", post(recipes::test_step))
        // W2.B Recipe authoring
        .route("/recipes/user", post(recipes::save_user))
        .route("/recipes/{id}/fork", post(recipes::fork))
        .route(
            "/recipes/user/{id}",
            axum::routing::delete(recipes::delete_user),
        )
        .route("/recipes/{id}/export", get(recipes::export_toml))
        .route("/recipes/import", post(recipes::import_toml))
        // Config management
        .route("/config", get(config_api::list_config))
        .route(
            "/config/{key}",
            get(config_api::get_config).put(config_api::set_config),
        )
        .route("/config/ai", post(config_api::set_ai_adapter))
        .route(
            "/config/ai/configure",
            post(config_api::configure_ai_adapter),
        )
        .route(
            "/config/connector/{name}",
            post(config_api::set_connector_config),
        )
        // Agents — state aggregation + autonomy
        .route("/agents/states", get(agents::list_states))
        .route(
            "/agents/{name}/autonomy",
            get(agents::get_autonomy).put(agents::set_autonomy),
        )
        .route("/agents/{name}/autonomy/step", post(agents::step_autonomy))
        // Author key registry
        .route("/authors", get(authors::list))
        .route(
            "/authors/{name}",
            post(authors::add).delete(authors::remove),
        )
        // Bot admin
        .route("/bot/status", get(bot::status))
        .route("/bot/formations", get(bot::formations))
        .route("/cooperation/utterances", get(utterances::utterance_defs))
        .route("/cooperation/utterances/recent", get(utterances::recent))
        .route("/bot/memory", get(bot::memory))
        // Data management
        .route("/data/export", post(data::export_data))
        .route("/data/import", post(data::import_data))
        .route("/data/purge", post(data::purge_data))
        // Memory management
        .route("/memory/audit", post(memory::audit_memory))
        .route("/memory/compact", post(memory::compact_memory))
        // Approval queue — blocking gate for dangerous capabilities
        // (currently `Capability::ShellExec`; OpenClaw 1-click-RCE
        // class). See `springtale-runtime::approval` + Phase-7 audit
        // Finding A.
        .route("/approvals", get(approvals::list_pending))
        .route("/approvals/{id}", post(approvals::resolve))
        .route("/chat", post(chat::send))
        // One-time ticket for the SSE routes below (plan 0.7).
        .route("/stream/ticket", post(auth::issue_stream_ticket))
        .layer(middleware::from_fn(auth::require_csrf_protection))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    // RateLimitLayer is wrapped by BufferLayer because tower::limit::RateLimit
    // does not implement Clone (required by axum). BufferLayer fronts the rate
    // limiter with a channel-based buffer whose handle is Clone. In
    // ServiceBuilder, layers compose outside-in: Trace → Buffer → RateLimit.
    // Dashboard SPA — embedded in binary via rust-embed.
    // In debug: loaded from filesystem (live reload). In release: baked into binary.
    // No path configuration needed — works from any directory.
    let dashboard = Router::new()
        .route("/ui", get(dashboard::serve_dashboard_index))
        .route("/ui/{*path}", get(dashboard::serve_dashboard));

    // SSE routes — EventSource cannot send headers, so these take a
    // one-time 30 s ticket (`POST /stream/ticket`, bearer-authenticated)
    // in the query string instead of a bearer token. Read-only GETs, so
    // no CSRF layer. `/stream` multiplexes events/canvas/cooperation;
    // `/chat/stream` stays separate because it is per-session.
    let streams = Router::new()
        .route("/stream", get(stream::stream))
        .route("/chat/stream", get(chat::stream))
        // POST: the connector config rides in the body, never the URL.
        .route("/workspaces/onboard", post(workspaces::onboard))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_stream_ticket,
        ));

    // WASM connector upload — same auth + CSRF gates as `authenticated`,
    // but exempt from the 1 MiB body limit below (tower-http's layer caps
    // every route it wraps; axum's `DefaultBodyLimit` alone can't raise it).
    let install_wasm = Router::new()
        .route("/connectors/install-wasm", post(connectors::install_wasm))
        .layer(DefaultBodyLimit::max(WASM_UPLOAD_LIMIT))
        .layer(RequestBodyLimitLayer::new(WASM_UPLOAD_LIMIT))
        .layer(middleware::from_fn(auth::require_csrf_protection))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    // Every other route sits under the 1 MiB request body limit.
    let limited = Router::new()
        .merge(public)
        .merge(authenticated)
        .merge(streams)
        .merge(dashboard)
        .layer(RequestBodyLimitLayer::new(1024 * 1024));

    Router::new()
        .merge(limited)
        .merge(install_wasm)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                // Security headers (ARCHITECTURE.md §9 dashboard security audit)
                .layer(SetResponseHeaderLayer::overriding(
                    header::X_FRAME_OPTIONS,
                    header::HeaderValue::from_static("DENY"),
                ))
                .layer(SetResponseHeaderLayer::overriding(
                    header::HeaderName::from_static("content-security-policy"),
                    header::HeaderValue::from_static(
                        "default-src 'self'; script-src 'self'; \
                         style-src 'self' 'unsafe-inline'; \
                         connect-src 'self' http://127.0.0.1:*; \
                         img-src 'self' data:; \
                         frame-ancestors 'none'",
                    ),
                ))
                .layer(SetResponseHeaderLayer::overriding(
                    header::HeaderName::from_static("x-content-type-options"),
                    header::HeaderValue::from_static("nosniff"),
                ))
                .layer(SetResponseHeaderLayer::overriding(
                    header::HeaderName::from_static("referrer-policy"),
                    header::HeaderValue::from_static("no-referrer"),
                ))
                .layer(SetResponseHeaderLayer::overriding(
                    header::HeaderName::from_static("permissions-policy"),
                    header::HeaderValue::from_static(
                        "camera=(), microphone=(), geolocation=(), accelerometer=(), gyroscope=()",
                    ),
                ))
                // NOTE: HSTS (Strict-Transport-Security) deliberately omitted.
                // RFC 6797 §8.1: HSTS MUST NOT be sent over plain HTTP.
                // springtaled binds 127.0.0.1 without TLS by default.
                .layer(axum::error_handling::HandleErrorLayer::new(
                    |_err: tower::BoxError| async move { StatusCode::TOO_MANY_REQUESTS },
                ))
                .layer(BufferLayer::new(256))
                .layer(RateLimitLayer::new(rate_limit, Duration::from_secs(1)))
                // Per-request timeout: 30s (architecture doc §8.1)
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::SERVICE_UNAVAILABLE,
                    Duration::from_secs(30),
                )),
        )
        .with_state(state)
}
