use std::time::Instant;

use super::deposit::SurfaceStore;

/// Drop expired surfaces. Called once per tick by the cooperation runtime.
pub fn sweep(store: &SurfaceStore, now: Instant) {
    let snap = store.snapshot();
    let retained: Vec<_> = snap
        .iter()
        .filter(|s| s.expires.is_none_or(|exp| exp > now))
        .cloned()
        .collect();
    if retained.len() != snap.len() {
        store.replace(retained);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    use super::super::trait_::SurfaceDeposit;
    use super::super::types::SurfaceType;
    use super::*;
    use crate::cadence::AgentId;

    #[test]
    fn expires_old_surfaces() {
        let store = SurfaceStore::new();
        let agent = AgentId::new();
        store.deposit(
            agent,
            SurfaceType::Active {
                remaining: Duration::from_millis(0),
            },
            serde_json::json!({}),
            Some(Duration::from_millis(0)),
            None,
        );
        store.deposit(
            agent,
            SurfaceType::Substrate,
            serde_json::json!({}),
            None,
            None,
        );

        // Sweep well after the first deposit's expiry.
        sweep(&store, Instant::now() + Duration::from_secs(1));
        assert_eq!(store.len(), 1);
    }
}
