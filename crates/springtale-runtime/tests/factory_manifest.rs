//! Every registered connector factory exposes its static manifest without
//! instantiating the connector (plan finding 121).

// Link `springtale-runtime` so its `extern crate connector_*` declarations pull
// every first-party connector's `inventory` registration into this test binary
// (see the note in the crate's lib.rs; rust-lang/rust#47384).
extern crate springtale_runtime;

use springtale_connector::factory::FactoryEntry;

/// Number of first-party connector crates linked by `springtale-runtime`.
const FIRST_PARTY_CONNECTORS: usize = 15;

#[test]
fn test_factory_manifest_every_entry_matches_name_and_declares_surface() {
    let mut seen = 0;
    for entry in inventory::iter::<FactoryEntry> {
        let factory = entry.factory;
        let manifest = factory.manifest();
        assert_eq!(
            manifest.name,
            factory.name(),
            "manifest name must match factory name"
        );
        assert!(
            !manifest.actions.is_empty() || !manifest.triggers.is_empty(),
            "{} declares neither actions nor triggers",
            factory.name()
        );
        seen += 1;
    }
    assert_eq!(
        seen, FIRST_PARTY_CONNECTORS,
        "unexpected number of registered factories"
    );
}
