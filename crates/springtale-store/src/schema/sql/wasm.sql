-- WASM connector binaries — persisted for reload across restarts.
-- Content-addressed: wasm_hash (SHA-256) verifies integrity on every
-- load. manifest_json includes the Ed25519 signature for author
-- verification (see docs/arch/SECURITY.md §3 and §9).
CREATE TABLE IF NOT EXISTS wasm_binaries (
    name          TEXT PRIMARY KEY,
    wasm_bytes    BLOB NOT NULL,
    manifest_json TEXT NOT NULL,
    wasm_hash     TEXT NOT NULL,
    author        TEXT NOT NULL,
    installed_at  TEXT NOT NULL
);
