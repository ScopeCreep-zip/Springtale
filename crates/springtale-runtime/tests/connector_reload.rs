//! Hot reload keeps working now that a connector name is registered
//! once (plan 0.10): `reload_connector` removes the installed entry
//! before re-installing the rebuilt connector under the same name.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use springtale_runtime::operations::connectors::{reload_connector, setup_connector};
use springtale_runtime::{RuntimeConfig, RuntimeState, StoreConfig};

/// A first-party connector that needs no config, so `setup_connector`
/// and the reload's factory rebuild both succeed with `{}`.
const NAME: &str = "connector-filesystem";

/// Boot the shared runtime over an ephemeral in-memory store with the
/// default NoopAdapter (no AI configured).
async fn boot() -> RuntimeState {
    let config = RuntimeConfig {
        store: StoreConfig {
            ephemeral: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (formation_cmd_tx, _formation_cmd_rx) = tokio::sync::mpsc::channel(16);
    springtale_runtime::init(&config, formation_cmd_tx, None, None)
        .await
        .expect("runtime init")
}

#[tokio::test]
async fn reload_of_installed_connector_succeeds() {
    let state = boot().await;

    let installed = setup_connector(&state, NAME, serde_json::json!({}))
        .await
        .expect("setup installs the connector");
    assert_eq!(installed, NAME);

    reload_connector(&state, NAME)
        .await
        .expect("reload of an installed connector succeeds");

    let registry = state.registry.read().await;
    let entry = registry
        .get(NAME)
        .expect("connector is still registered after reload");
    assert!(entry.enabled, "reload preserves the enabled flag");
    assert_eq!(
        registry.list().len(),
        1,
        "reload replaces the entry rather than adding a second one"
    );
}
