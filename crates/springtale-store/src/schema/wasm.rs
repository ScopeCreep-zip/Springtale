/// Row in the `wasm_binaries` table — persisted WASM connector binary.
///
/// Content-addressed: `wasm_hash` (SHA-256) is verified on every load.
/// The manifest includes an Ed25519 signature for author verification.
#[derive(Debug, Clone)]
pub struct WasmBinaryRow {
    /// Connector name (e.g., "connector-kick-community").
    pub name: String,
    /// Compiled WASM binary bytes.
    pub wasm_bytes: Vec<u8>,
    /// Serialized ConnectorManifest JSON.
    pub manifest_json: String,
    /// SHA-256 hash of the WASM binary (hex).
    pub wasm_hash: String,
    /// Author name from the manifest.
    pub author: String,
    /// When this connector was installed.
    pub installed_at: chrono::DateTime<chrono::Utc>,
}
