//! Container healthcheck subcommand.
//!
//! Probes the daemon's `/health` endpoint. Exit code 0 means the daemon
//! is up and responsive; any non-zero code is a failure that the container
//! runtime should react to (restart, alert, mark unhealthy).
//!
//! Lives in the CLI binary specifically so it ships inside the distroless
//! container image — no separate `wget`/`curl` dependency.

use std::time::Duration;

use anyhow::{Result, anyhow};

use crate::output;

/// Liveness: the process is up.
const HEALTH: &str = "/health";
/// Readiness: the process is up *and* willing to serve.
const READY: &str = "/ready";

pub async fn run(base_url: &str, ready: bool, json_out: bool) -> Result<()> {
    let client = springtale_transport::safe_http::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| anyhow!("healthcheck client: {e}"))?;

    let probe = if ready { READY } else { HEALTH };
    let url = format!("{}{probe}", base_url.trim_end_matches('/'));
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("healthcheck request: {e}"))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "healthcheck failed: HTTP {}",
            response.status().as_u16()
        ));
    }
    // A healthy probe stays silent for the container runtime; `--json`
    // gives a scriptable body without changing the exit-code contract.
    let body = serde_json::json!({ "healthy": true, "url": url, "probe": probe });
    output::emit_status(json_out, &body, |_| String::new())
}
