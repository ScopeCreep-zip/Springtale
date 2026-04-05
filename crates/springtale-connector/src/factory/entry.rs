use super::trait_::ConnectorFactory;

/// Registry entry for compile-time connector discovery.
///
/// Each connector crate submits one of these via `inventory::submit!`.
/// The runtime iterates `inventory::iter::<FactoryEntry>` at startup
/// to discover all compiled-in connectors.
pub struct FactoryEntry {
    pub factory: &'static dyn ConnectorFactory,
}

inventory::collect!(FactoryEntry);
