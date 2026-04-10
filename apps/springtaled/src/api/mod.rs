pub mod agents;
pub mod auth;
pub mod authors;
pub mod extractors;
pub mod bot;
pub mod canvas;
pub mod canvas_stream;
pub mod config_api;
pub mod connectors;
pub mod dashboard;
pub mod data;
pub mod events;
pub mod events_stream;
pub mod formations;
pub mod health;
pub mod memory;
pub mod rules;
pub mod safety;
pub mod sessions;
pub mod state;
pub mod webhooks;

/// Maximum length for API path parameters. Prevents DoS via oversized route strings.
const MAX_PATH_SEGMENT_LEN: usize = 256;

/// Validate that a path parameter is within acceptable length.
pub fn validate_path_param(param: &str) -> Result<(), axum::http::StatusCode> {
    if param.len() > MAX_PATH_SEGMENT_LEN {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    Ok(())
}

use std::time::Duration;

use axum::Router;
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
        .route("/connectors/{name}/cascade", delete(connectors::remove_cascade))
        .route("/connectors/{name}/config", get(connectors::get_config))
        .route("/connectors/{name}/outputs", get(connectors::list_outputs))
        .route("/connectors/{name}/enable", post(connectors::enable))
        .route("/connectors/{name}/disable", post(connectors::disable))
        .route("/connectors/{name}/test", post(rules::test_connector))
        .route("/connectors/{name}/upsert-config", post(config_api::upsert_connector_config))
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
        .route("/events/stream", get(events_stream::stream))
        .route("/sessions", get(sessions::list))
        .route(
            "/config/heartbeat",
            get(config_api::get_heartbeat).put(config_api::set_heartbeat),
        )
        .route("/canvas", get(canvas::get_canvas))
        .route("/canvas/update", post(canvas::update_canvas))
        .route("/canvas/stream", get(canvas_stream::stream))
        .route("/webhook/{connector}/{trigger}", post(webhooks::receive))
        .route("/formations", get(formations::list).post(formations::create))
        .route("/formations/{id}/deploy", post(formations::deploy))
        .route("/formations/{id}/pause", post(formations::pause))
        .route("/formations/{id}/resume", post(formations::resume))
        .route("/formations/{id}/dissolve", post(formations::dissolve))
        .route("/formations/{id}/intent", put(formations::update_intent))
        .route("/formations/{id}/members", post(formations::add_member))
        .route("/formations/intents", get(formations::list_intents))
        .route("/formations/deploy-team", post(formations::deploy_team))
        .route("/formations/{id}/cycle-intent", post(formations::cycle_intent))
        .route("/formations/{id}/cycle-autonomy", post(formations::cycle_autonomy))
        .route("/formations/{id}/toggle-guard", post(config_api::toggle_formation_guard))
        .route("/safety", get(safety::get_config).put(safety::save_config))
        // Config management
        .route("/config", get(config_api::list_config))
        .route("/config/{key}", get(config_api::get_config).put(config_api::set_config))
        .route("/config/ai", post(config_api::set_ai_adapter))
        .route("/config/ai/configure", post(config_api::configure_ai_adapter))
        .route("/config/connector/{name}", post(config_api::set_connector_config))
        // Agents — state aggregation + autonomy
        .route("/agents/states", get(agents::list_states))
        .route("/agents/{name}/autonomy", get(agents::get_autonomy).put(agents::set_autonomy))
        .route("/agents/{name}/autonomy/step", post(agents::step_autonomy))
        // Author key registry
        .route("/authors", get(authors::list))
        .route("/authors/{name}", post(authors::add).delete(authors::remove))
        // Bot admin
        .route("/bot/status", get(bot::status))
        .route("/bot/formations", get(bot::formations))
        .route("/bot/memory", get(bot::memory))
        // Data management
        .route("/data/export", post(data::export_data))
        // Memory management
        .route("/memory/audit", post(memory::audit_memory))
        .route("/memory/compact", post(memory::compact_memory))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    // RateLimitLayer is wrapped by BufferLayer because tower::limit::RateLimit
    // does not implement Clone (required by axum). BufferLayer fronts the rate
    // limiter with a channel-based buffer whose handle is Clone. In
    // ServiceBuilder, layers compose outside-in: Trace → BodyLimit → Buffer → RateLimit.
    // Dashboard SPA — embedded in binary via rust-embed.
    // In debug: loaded from filesystem (live reload). In release: baked into binary.
    // No path configuration needed — works from any directory.
    let dashboard = Router::new()
        .route("/ui", get(dashboard::serve_dashboard_index))
        .route("/ui/{*path}", get(dashboard::serve_dashboard));

    Router::new()
        .merge(public)
        .merge(authenticated)
        .merge(dashboard)
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
                .layer(RequestBodyLimitLayer::new(1024 * 1024))
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
