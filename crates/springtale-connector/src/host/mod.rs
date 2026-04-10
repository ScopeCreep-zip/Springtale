//! Connector host abstraction — shared interface for native and WASM execution.
//!
//! Both `NativeConnectorHost` and `WasmConnectorHost` implement this trait.
//! The registry stores `Arc<dyn ConnectorHost>` so the dispatch path is
//! identical regardless of execution model.

pub mod trait_;

pub use trait_::ConnectorHost;
