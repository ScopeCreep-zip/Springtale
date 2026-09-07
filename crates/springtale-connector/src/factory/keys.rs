//! The config keys the compile-time factory registry declares.
//!
//! A headless deployment configures connectors from a TOML file, and the
//! daemon has to know which top-level tables in that file are connector
//! config. It used to know by holding a hand-written list of names, so a
//! connector added after that list was written could not be configured
//! from the file at all. The list is derived from the registry instead:
//! every factory that is compiled in declares its own key.

use super::entry::FactoryEntry;

/// Every `config_key` declared by a compiled-in connector factory,
/// sorted and deduplicated.
///
/// Empty when no connector crate is linked into the binary — the
/// factories register themselves through `inventory::submit!`, so only
/// linked crates appear.
#[must_use]
pub fn config_keys() -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = inventory::iter::<FactoryEntry>
        .into_iter()
        .map(|entry| entry.factory.config_key())
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// No connector crate depends on this one in reverse, so this crate's
    /// own test binary links no factories: the contract under test is the
    /// shape (sorted, deduplicated), not the contents. The daemon's test
    /// suite covers the populated case.
    #[test]
    fn test_config_keys_are_sorted_and_unique() {
        let keys = config_keys();
        let mut expected = keys.clone();
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(keys, expected);
    }
}
