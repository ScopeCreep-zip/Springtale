pub mod auth;
pub mod connectors;
pub mod events;
pub mod health;
pub mod rules;
pub mod state;
pub mod webhooks;

use std::time::Duration;

use axum::http::StatusCode;
use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::Router;
use tower::buffer::BufferLayer;
use tower::limit::RateLimitLayer;
use tower::ServiceBuilder;
use tower_http::limit::RequestBodyLimitLayer;
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
        .route("/connectors/install", post(connectors::install))
        .route("/connectors/{name}", delete(connectors::remove))
        .route("/connectors/{name}/enable", post(connectors::enable))
        .route("/connectors/{name}/disable", post(connectors::disable))
        .route("/rules", get(rules::list).post(rules::create))
        .route("/rules/{id}", put(rules::update).delete(rules::delete))
        .route("/rules/{id}/run", post(rules::run))
        .route("/events", get(events::list))
        .route(
            "/webhook/{connector}/{trigger}",
            post(webhooks::receive),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    // RateLimitLayer is wrapped by BufferLayer because tower::limit::RateLimit
    // does not implement Clone (required by axum). BufferLayer fronts the rate
    // limiter with a channel-based buffer whose handle is Clone. In
    // ServiceBuilder, layers compose outside-in: Trace → BodyLimit → Buffer → RateLimit.
    Router::new()
        .merge(public)
        .merge(authenticated)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(RequestBodyLimitLayer::new(1024 * 1024))
                .layer(axum::error_handling::HandleErrorLayer::new(
                    |_err: tower::BoxError| async move {
                        StatusCode::TOO_MANY_REQUESTS
                    },
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
