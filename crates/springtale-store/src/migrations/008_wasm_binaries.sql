-- WASM connector binaries — persisted for reload across restarts.
-- Content-addressed: wasm_hash (SHA-256) verifies integrity on every load.
-- Manifest includes Ed25519 signature for author verification.

CREATE TABLE IF NOT EXISTS wasm_binaries (
    name TEXT PRIMARY KEY,
    wasm_bytes BLOB NOT NULL,
    manifest_json TEXT NOT NULL,
    wasm_hash TEXT NOT NULL,
    author TEXT NOT NULL,
    installed_at TEXT NOT NULL
);
