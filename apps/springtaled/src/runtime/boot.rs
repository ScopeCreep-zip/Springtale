use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, RwLock};

use springtale_connector::capability::grant::CapabilityPolicy;
use springtale_connector::registry::store::ConnectorRegistry;
use springtale_core::rule::engine::RuleEngine;
use springtale_crypto::identity::keypair::Keypair;
use springtale_crypto::vault::store::Vault;
use springtale_scheduler::cron::executor::CronExecutor;
use springtale_scheduler::queue::producer::JobProducer;
use springtale_scheduler::queue::consumer::JobConsumer;
use springtale_scheduler::watcher::fs_watcher::FsWatcher;
use springtale_store::backend::sqlite::SqliteBackend;
use springtale_store::backend::trait_::StorageBackend;
use springtale_transport::local::unix_socket::LocalTransport;

use crate::api;
use crate::config::SpringtaleConfig;

/// Boot the springtaled daemon.
///
/// Executes the ordered startup sequence from the architecture doc (§8.1).
/// Each step must succeed before the next. Errors are fatal.
pub async fn boot(config: SpringtaleConfig) -> Result<()> {
    // ── Step 1: Config already loaded by caller ──
    tracing::info!("springtaled starting");

    // Warn if API is bound to 0.0.0.0
    if config.api.bind.starts_with("0.0.0.0") {
        tracing::warn!(
            bind = %config.api.bind,
            "management API bound to all interfaces — this exposes it to the network"
        );
    }

    // ── Step 2: Initialize store (SQLite, auto-migrations) ──
    tracing::info!(path = %config.store.path.display(), "opening SQLite store");
    if let Some(parent) = config.store.path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create data directory: {}", parent.display()))?;
    }
    let store = Arc::new(
        SqliteBackend::open(&config.store.path)
            .context("failed to open SQLite store")?,
    );
    tracing::info!("store initialized");

    // ── Step 3: Initialize crypto vault ──
    tracing::info!(path = %config.crypto.vault_path.display(), "opening crypto vault");
    if let Some(parent) = config.crypto.vault_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create vault directory: {}", parent.display()))?;
    }

    let passphrase = get_passphrase()?;
    let (vault, keypair) = open_or_create_vault(&config.crypto.vault_path, &passphrase)?;
    let node_id = keypair.node_id();
    tracing::info!(node_id = %hex::encode(node_id.as_bytes()), "identity loaded");

    // Derive API token from passphrase hash (HMAC-SHA256)
    let api_token_hash = derive_api_token_hash(&passphrase);

    // ── Step 4: Initialize transport ──
    tracing::info!(path = %config.transport.socket_path.display(), "binding local transport");
    let _transport = Arc::new(
        LocalTransport::bind(node_id, &config.transport.socket_path)
            .await
            .context("failed to bind local transport")?,
    );
    tracing::info!("transport initialized");

    // ── Step 5: Load rules from store → RuleEngine ──
    let rules = store.list_rules().await
        .context("failed to load rules from store")?;
    let mut engine = RuleEngine::new();
    for rule in &rules {
        engine.add_rule(rule.clone());
    }
    tracing::info!(rules = rules.len(), "rule engine loaded");

    // ── Step 6: Start scheduler (cron + file watcher) ──
    let (trigger_tx, trigger_rx) = mpsc::channel(256);

    let mut cron_executor = CronExecutor::new(trigger_tx.clone());
    let mut fs_watcher = FsWatcher::new(trigger_tx.clone())
        .context("failed to create filesystem watcher")?;

    // Schedule cron triggers and file watches from rules
    for rule in &rules {
        if let springtale_core::rule::Trigger::Cron { expression, .. } = &rule.trigger
            && let Err(e) = cron_executor.schedule(&rule.name, expression)
        {
            tracing::warn!(rule = %rule.name, error = %e, "failed to schedule cron trigger");
        }
        if let springtale_core::rule::Trigger::FileWatch { path, .. } = &rule.trigger
            && let Err(e) = fs_watcher.watch(path)
        {
            tracing::warn!(rule = %rule.name, error = %e, "failed to watch path");
        }
    }
    tracing::info!(
        cron_jobs = cron_executor.list().len(),
        watched_paths = fs_watcher.watched_paths().len(),
        "scheduler started"
    );

    // ── Step 7: Load connectors → ConnectorRegistry ──
    let registry = Arc::new(RwLock::new(
        ConnectorRegistry::new(CapabilityPolicy::Interactive),
    ));

    // Load installed connectors from store and log them.
    // Phase 1a: manifests are registered in the store (via CLI or API) but
    // connectors can't be activated from manifests alone — they require compiled
    // Rust code. The registry is populated when the daemon is built with
    // connector crates as dependencies (Phase 2 dynamic loading).
    let installed_connectors = store.list_connectors().await
        .context("failed to load connectors from store")?;
    let enabled_count = installed_connectors.iter().filter(|c| c.enabled).count();
    for connector in &installed_connectors {
        tracing::info!(
            name = %connector.name,
            version = %connector.version,
            enabled = connector.enabled,
            "found installed connector"
        );
    }
    tracing::info!(
        total = installed_connectors.len(),
        enabled = enabled_count,
        "connector registry initialized"
    );

    // ── Step 7b: Initialize job queue (action execution pipeline) ──
    let (job_tx, job_rx) = mpsc::channel(100);
    let producer = Arc::new(JobProducer::new(job_tx));
    let mut consumer = JobConsumer::new(job_rx, 4);

    // Install action dispatcher as the job handler.
    // Jobs contain serialized Action payloads. The handler deserializes
    // and dispatches each action via dispatch::dispatch_action().
    let dispatch_registry = registry.clone();
    consumer.set_handler(std::sync::Arc::new(move |job| {
        let reg = dispatch_registry.clone();
        Box::pin(async move {
            let action: springtale_core::rule::action::Action =
                serde_json::from_value(job.payload)
                    .map_err(|e| format!("failed to deserialize action: {e}"))?;
            crate::dispatch::dispatch_action(&action, &reg).await.map(|_| ())
        })
    }));

    // Spawn consumer as background task
    tokio::spawn(async move {
        if let Err(e) = consumer.run().await {
            tracing::error!(error = %e, "job consumer error");
        }
    });
    tracing::info!("job queue started (concurrency: 4)");

    // ── Step 8: Build and start API server ──
    let engine = Arc::new(RwLock::new(engine));

    let ready_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let cron_arc = Arc::new(tokio::sync::Mutex::new(cron_executor));
    let watcher_arc = Arc::new(tokio::sync::Mutex::new(fs_watcher));

    let state = api::state::AppState {
        store: store.clone(),
        registry: registry.clone(),
        engine: engine.clone(),
        api_token_hash,
        ready: ready_flag.clone(),
        trigger_tx: trigger_tx.clone(),
        cron: cron_arc,
        fs_watcher: watcher_arc,
        rate_limit_per_sec: u64::from(config.api.rate_limit_per_sec),
    };

    let router = api::build_router(state);
    let listener = tokio::net::TcpListener::bind(&config.api.bind)
        .await
        .with_context(|| format!("failed to bind API to {}", config.api.bind))?;
    tracing::info!(bind = %config.api.bind, "management API listening");

    // ── Step 9: Signal readiness ──
    ready_flag.store(true, std::sync::atomic::Ordering::Release);
    println!("READY");

    // ── Run: API server + trigger event loop ──
    let api_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router)
            .with_graceful_shutdown(crate::shutdown::shutdown_signal())
            .await
        {
            tracing::error!(error = %e, "API server error");
        }
    });

    let event_loop = tokio::spawn(async move {
        event_loop(trigger_rx, engine, producer).await;
    });

    // Wait for shutdown
    tokio::select! {
        _ = api_handle => tracing::info!("API server stopped"),
        _ = event_loop => tracing::info!("event loop stopped"),
    }

    // Cleanup
    drop(vault);
    tracing::info!("springtaled shutdown complete");
    Ok(())
}

/// Main event loop: receives trigger events, matches rules, enqueues actions.
///
/// Uses `router::dispatch_event` (from springtale-core) to match triggers
/// against rules, then serializes each matched action as a Job and enqueues
/// it via the JobProducer. The JobConsumer processes jobs asynchronously
/// through `dispatch::dispatch_action`.
async fn event_loop(
    mut trigger_rx: mpsc::Receiver<springtale_core::rule::engine::TriggerEvent>,
    engine: Arc<RwLock<RuleEngine>>,
    producer: Arc<JobProducer>,
) {
    while let Some(event) = trigger_rx.recv().await {
        let engine = engine.read().await;
        let matches = springtale_core::router::dispatch::dispatch_event(&engine, &event);

        for rule_match in &matches {
            tracing::info!(
                rule = %rule_match.rule_name,
                actions = rule_match.actions.len(),
                "rule matched trigger — enqueuing actions"
            );

            for action in &rule_match.actions {
                match serde_json::to_value(action) {
                    Ok(payload) => {
                        if let Err(e) = producer.enqueue(payload, 3).await {
                            tracing::error!(
                                rule = %rule_match.rule_name,
                                error = %e,
                                "failed to enqueue action"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            rule = %rule_match.rule_name,
                            error = %e,
                            "failed to serialize action"
                        );
                    }
                }
            }
        }
    }
}

/// Get the vault passphrase from environment or interactive prompt.
fn get_passphrase() -> Result<Vec<u8>> {
    // Try environment variable first (for daemon/Docker deployment)
    if let Ok(pass) = std::env::var("SPRINGTALE_PASSPHRASE") {
        return Ok(pass.into_bytes());
    }

    // Interactive prompt if stdin is a terminal
    if atty_check() {
        let pass = rpassword::read_password_from_tty(Some("Vault passphrase: "))
            .context("failed to read passphrase")?;
        if pass.is_empty() {
            anyhow::bail!("passphrase cannot be empty");
        }
        return Ok(pass.into_bytes());
    }

    anyhow::bail!(
        "no passphrase provided: set SPRINGTALE_PASSPHRASE env var or run interactively"
    )
}

/// Check if stdin is a terminal (for interactive passphrase prompt).
fn atty_check() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Open an existing vault or create a new one on first run.
fn open_or_create_vault(
    path: &std::path::Path,
    passphrase: &[u8],
) -> Result<(Vault, Keypair)> {
    if path.exists() {
        let vault = Vault::open(path, passphrase)
            .context("failed to open vault (wrong passphrase?)")?;
        let identity_bytes = vault
            .get("identity")
            .context("failed to read identity from vault")?
            .ok_or_else(|| anyhow::anyhow!("vault has no identity key"))?
            .clone();
        let bytes: [u8; 32] = identity_bytes
            .as_slice()
            .try_into()
            .context("identity key is wrong size (expected 32 bytes)")?;
        let keypair = Keypair::from_secret_bytes(bytes)
            .context("failed to restore keypair from vault")?;
        Ok((vault, keypair))
    } else {
        tracing::info!("creating new vault and identity");
        let keypair = Keypair::generate()
            .context("failed to generate identity keypair")?;
        let mut vault = Vault::create(path, passphrase)
            .context("failed to create vault")?;
        // SECURITY: expose needed to persist identity key material
        vault
            .set("identity", keypair.expose_secret_bytes().to_vec())
            .context("failed to store identity in vault")?;
        vault.save().context("failed to save vault")?;
        Ok((vault, keypair))
    }
}

/// Derive an API authentication token hash from the vault passphrase.
///
/// Uses HMAC-SHA256(passphrase, "springtale-api-token") as the token.
/// The management API compares incoming Bearer tokens against this hash.
fn derive_api_token_hash(passphrase: &[u8]) -> [u8; 32] {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    // HMAC-SHA256 accepts any key size — this cannot fail.
    #[allow(clippy::expect_used)]
    let mut mac = HmacSha256::new_from_slice(passphrase)
        .expect("HMAC-SHA256 accepts any key size");
    mac.update(b"springtale-api-token");
    let result = mac.finalize();
    let bytes = result.into_bytes();

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    hash
}
