-- Sentinel audit trail (append-only).
-- Every action that a connector takes — successful or refused —
-- gets a row here. verdict is the Sentinel's go/refuse decision;
-- result is the connector's reply payload.
CREATE TABLE IF NOT EXISTS audit_trail (
    id              TEXT PRIMARY KEY,
    timestamp       TEXT NOT NULL,
    connector_name  TEXT NOT NULL,
    action_type     TEXT NOT NULL,
    action_summary  TEXT NOT NULL DEFAULT '',
    verdict         TEXT NOT NULL DEFAULT 'go',
    verdict_reason  TEXT NOT NULL DEFAULT '',
    result          TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_trail_timestamp ON audit_trail(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_trail_connector ON audit_trail(connector_name);
CREATE INDEX IF NOT EXISTS idx_audit_trail_verdict   ON audit_trail(verdict);
