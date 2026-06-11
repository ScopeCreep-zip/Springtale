-- Sentinel audit trail (append-only + tamper-evident).
-- Every action that a connector takes — successful or refused —
-- gets a row here. verdict is the Sentinel's go/refuse decision;
-- result is the connector's reply payload.
--
-- Phase-7 audit Finding B: each row carries a SHA-256 hash that
-- chains to the previous row's hash. `row_hash = SHA-256(prev_hash
-- || canonical_row_json)` where `canonical_row_json` is the sorted-
-- key JSON of every column EXCEPT the chain columns themselves.
-- The verifier walks the chain on daemon startup; any tampering
-- (mutate, delete, reorder) breaks the chain and fails to start.
--
-- Genesis anchor (`prev_hash` of the first row) is the SHA-256 of
-- the vault identity key's public bytes, so the chain is bound to
-- the vault — a fresh SQLite + same vault picks up where the
-- previous chain left off; a fresh SQLite + different vault starts
-- a new chain.
CREATE TABLE IF NOT EXISTS audit_trail (
    id              TEXT PRIMARY KEY,
    timestamp       TEXT NOT NULL,
    connector_name  TEXT NOT NULL,
    action_type     TEXT NOT NULL,
    action_summary  TEXT NOT NULL DEFAULT '',
    verdict         TEXT NOT NULL DEFAULT 'go',
    verdict_reason  TEXT NOT NULL DEFAULT '',
    result          TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL,
    -- SHA-256 hex of the previous row's row_hash (or vault-identity
    -- hash for the genesis row). 64 hex chars.
    prev_hash       TEXT NOT NULL DEFAULT '',
    -- SHA-256 hex of `prev_hash || canonical_row_json`. 64 hex chars.
    row_hash        TEXT NOT NULL DEFAULT '',
    -- Monotonic insert order — used by the verifier to walk the
    -- chain. Separate from id (UUID, unordered) and timestamp (can
    -- collide on fast inserts). Auto-assigned on INSERT.
    chain_seq       INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_trail_timestamp ON audit_trail(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_trail_connector ON audit_trail(connector_name);
CREATE INDEX IF NOT EXISTS idx_audit_trail_verdict   ON audit_trail(verdict);
-- chain_seq is the verifier's walk order; unique + indexed.
CREATE UNIQUE INDEX IF NOT EXISTS idx_audit_trail_chain_seq ON audit_trail(chain_seq);
