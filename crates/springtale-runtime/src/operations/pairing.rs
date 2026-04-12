//! Pairing operations — shared between CLI and API.
//!
//! Generates pairing codes on the trusted device (daemon host) and
//! revokes all paired users for IPV panic scenarios.

use springtale_store::StorageBackend;

use crate::error::OperationError;

/// Generate a pairing code and store it in the config_store with a
/// 10-minute TTL. Returns the code so the CLI can display it.
///
/// The code is NEVER sent via chat — it's shown on the daemon's
/// terminal or dashboard. The user types it into their chat client.
/// This matches Signal/WhatsApp's direction-of-trust: the secret
/// originates on the trusted device.
pub async fn generate_pairing_code(store: &dyn StorageBackend) -> Result<String, OperationError> {
    let code = gen_code();
    let key = format!("pairing_code:{code}");
    let val = serde_json::json!({
        "created_at": chrono::Utc::now().to_rfc3339(),
        "generated_on": "daemon_host",
    })
    .to_string();
    store
        .set_config(&key, &val)
        .await
        .map_err(OperationError::Store)?;
    Ok(code)
}

/// Revoke ALL paired users and invalidate ALL outstanding pairing codes.
///
/// Critical for IPV users: if the phone is seized, the abuser has chat
/// access and can see paired users. This command wipes everything
/// without needing chat access — the admin runs it on the server.
pub async fn panic_unpair(store: &dyn StorageBackend) -> Result<u32, OperationError> {
    let all = store.list_config().await.map_err(OperationError::Store)?;
    let mut removed = 0u32;
    for (key, _) in &all {
        if key.starts_with("paired:")
            || key.starts_with("pairing_code:")
            || key.starts_with("pairing_rate:")
        {
            let _ = store.delete_config(key).await;
            removed += 1;
        }
    }
    Ok(removed)
}

fn gen_code() -> String {
    use rand::RngCore;
    const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut bytes = [0u8; 10];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|b| CROCKFORD[(*b as usize) % CROCKFORD.len()] as char)
        .collect()
}
