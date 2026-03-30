//! Shared HTTP client utilities for connectors.
//!
//! Common response-handling logic lives here so individual connectors
//! don't duplicate the same parse-status-deserialize pattern.

/// Parse a JSON API response: check for HTTP error status, read body, deserialize.
///
/// Returns `Result<Value, String>` intentionally — each connector wraps
/// the `String` into its own typed error enum via `.map_err()`.
pub async fn handle_json_response(
    response: reqwest::Response,
) -> Result<serde_json::Value, String> {
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| format!("failed to read response: {e}"))?;

    if status >= 400 {
        return Err(format!("API returned {status}: {body}"));
    }

    serde_json::from_str(&body).map_err(|e| format!("failed to parse JSON response: {e}"))
}

// Unit tests for `handle_json_response` require constructing a `reqwest::Response`
// from an `http::Response`, which needs the `http` crate as a dev-dependency.
// Rather than adding a dependency solely for tests, this function is tested
// transitively through every connector that calls it (kick, github, etc.).
