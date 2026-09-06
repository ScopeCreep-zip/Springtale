mod bot;
mod crypto;
mod formations;
pub mod options;
pub mod pipeline;
mod sentinel;
mod transport;

use std::sync::Arc;

use anyhow::{Context, Result};
use secrecy::SecretString;

use crate::api;
use crate::api::lock::{Live, RebuildFuture, RuntimeGuard};
use crate::config::SpringtaleConfig;

/// Boot the springtaled daemon.
///
/// Executes the ordered startup sequence from the architecture doc (§8.1).
/// Each step must succeed before the next. Errors are fatal.
///
/// Steps 2 through 8 live in [`pipeline::build_live`] rather than here,
/// because `POST /vault/unlock` runs exactly the same sequence after a
/// lock has dropped the previous one (plan 6.10).
pub async fn boot(
    config: SpringtaleConfig,
    connector_configs: std::collections::HashMap<String, serde_json::Value>,
    options: options::BootOptions,
) -> Result<()> {
    // ── Step 1: Config already loaded by caller ──
    tracing::info!("springtaled starting");

    // `--bind` (sidecar / mobile in-process boot) overrides `[api] bind`.
    let bind_addr = options
        .bind
        .clone()
        .unwrap_or_else(|| config.api.bind.clone());

    // Warn if API is bound to 0.0.0.0
    if bind_addr.starts_with("0.0.0.0") {
        tracing::warn!(
            bind = %bind_addr,
            "management API bound to all interfaces — this exposes it to the network"
        );
    }

    // Destructure config to avoid partial-move issues (ai/sentinel fields
    // are moved into RuntimeConfig, the rest stays available by name).
    let SpringtaleConfig {
        ephemeral,
        store,
        crypto: crypto_config,
        transport,
        api: api_config,
        heartbeat_interval_secs,
        sentinel,
    } = config;

    let ctx = Arc::new(pipeline::BootContext {
        ephemeral,
        store,
        crypto: crypto_config,
        transport,
        api: api_config,
        heartbeat_interval_secs,
        sentinel,
        connector_configs,
    });

    // ── Steps 2–8: open the vault and build the world ──
    // The passphrase is read once here and zeroized on drop; an unlock
    // supplies its own from the request body.
    let live = {
        let passphrase = crypto::read_passphrase(options.passphrase_stdin)?;
        pipeline::build_live(&ctx, &passphrase).await?
    };

    // ── Step 8b: the lock ──
    // The router served below is the OUTER one: `GET /health`,
    // `GET /ready` and `POST /vault/unlock` always answer, everything
    // else is forwarded to the live router or refused with 503. Locking
    // drops `Live` — and with it the store handle and the vault key —
    // without the process exiting.
    let guard = RuntimeGuard::new(live, rebuild_from(ctx));
    let auto_lock = guard.spawn_auto_lock();
    let router = api::lock::build_outer_router(guard.clone());

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind API to {bind_addr}"))?;
    // `--bind 127.0.0.1:0` asks the OS for an ephemeral port, so the
    // bound address — not the requested one — is what the parent needs.
    let bound = listener
        .local_addr()
        .context("failed to read the bound API address")?;
    tracing::info!(bind = %bound, "management API listening");

    // ── Step 9: Signal readiness ──
    // The desktop sidecar (plan 2.1) blocks on this exact line to learn
    // the port. Process supervisors that only matched the old bare
    // `READY` still match the prefix.
    println!("READY {}", bound.port());
    // A sidecar's stdout is a pipe, which is block-buffered: without an
    // explicit flush the parent would wait for the buffer to fill and
    // never see READY.
    use std::io::Write as _;
    let _ = std::io::stdout().flush();

    // ── Run: API server (cron + queue + event loop run inside the
    //         shared `bootstrap_embedded` from springtale-runtime) ──
    //
    // Only the shutdown signal ends this. The bot event loop stopping
    // no longer takes the daemon down with it: after a lock there is no
    // bot, and the daemon has to stay up to accept the unlock.
    if let Err(e) = axum::serve(listener, router)
        .with_graceful_shutdown(crate::shutdown::shutdown_signal())
        .await
    {
        tracing::error!(error = %e, "API server error");
    }
    tracing::info!("API server stopped");

    auto_lock.abort();

    // Shutdown is a lock: it signals every connector chat loop
    // (Telegram polling, Discord, IRC, ...) to drain its in-flight work
    // and exit, stops the scheduler, ends the background tasks, and
    // zeroizes the vault key — the same teardown `POST /vault/lock`
    // performs, so the two paths cannot drift.
    guard.lock().await;

    tracing::info!("springtaled shutdown complete");
    Ok(())
}

/// The unlock hook: re-run the pipeline over a caller-supplied passphrase.
fn rebuild_from(ctx: Arc<pipeline::BootContext>) -> api::lock::Rebuild {
    Arc::new(move |passphrase: SecretString| -> RebuildFuture {
        let ctx = ctx.clone();
        Box::pin(async move {
            let bytes = api::lock::expose_passphrase(&passphrase);
            let live: Live = pipeline::build_live(&ctx, bytes).await?;
            Ok(live)
        })
    })
}
