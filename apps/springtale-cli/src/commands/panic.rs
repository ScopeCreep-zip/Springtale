use anyhow::Result;

use springtale_store::backend::trait_::StorageBackend;

use crate::output;

/// Execute emergency data destruction.
///
/// NO confirmation prompt — this is an emergency command for IPV survivors
/// and activists who need to destroy data immediately. The user configured
/// this command knowing what it does.
///
/// Delegates to `springtale_runtime::operations::safety::panic_wipe` which:
/// 1. Overwrites vault file with random bytes, then deletes
/// 2. Overwrites SQLite database + WAL + SHM with random bytes, then deletes
/// 3. Overwrites config file (may contain connector settings that reveal activity)
pub async fn run(store: &dyn StorageBackend, json_out: bool) -> Result<()> {
    springtale_runtime::operations::safety::panic_wipe(store)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let body = wiped_body();
    output::emit_status(json_out, &body, |_| "All data destroyed.".to_owned())
}

/// The `--json` body the panic wipe emits. One field, so a script can
/// tell a completed wipe from a failed one without parsing prose.
fn wiped_body() -> serde_json::Value {
    serde_json::json!({ "wiped": true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    #[test]
    fn test_panic_json_shape_is_a_single_wiped_flag() {
        let out = json_value(&wiped_body());
        assert_eq!(key_set(&out), ["wiped"]);
        assert!(out["wiped"].is_boolean());
        assert_eq!(out["wiped"], true);
    }
}
