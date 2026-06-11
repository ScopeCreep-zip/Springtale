/// Row in the `wasm_binaries` table — persisted WASM connector binary.
///
/// Content-addressed: `wasm_hash` (SHA-256) is verified on every load
/// (R-004 in `docs/security/RISK-REGISTER.md`). The manifest includes
/// an Ed25519 signature, AND the install-time signing pubkey + the
/// install-time signature are pinned in the store separately
/// (`author_pubkey_hex`, `manifest_sig_hex`) so a swap-the-whole-bundle
/// attack against the SQLite store cannot bypass re-verification. TUF
/// §4 trust-anchor separation.
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
    /// Install-time signing key (hex-encoded Ed25519 public key, 64
    /// chars). Empty on pre-v8-migration rows; the boot re-verifier
    /// treats empty as a legacy install and logs a warning rather
    /// than failing closed, so an existing deployment isn't bricked
    /// by the audit fix.
    pub author_pubkey_hex: String,
    /// Install-time Ed25519 signature over the canonical manifest
    /// JSON minus the `signature` field (hex, 128 chars). Empty on
    /// pre-v8-migration rows.
    pub manifest_sig_hex: String,
    /// When this connector was installed.
    pub installed_at: chrono::DateTime<chrono::Utc>,
}
