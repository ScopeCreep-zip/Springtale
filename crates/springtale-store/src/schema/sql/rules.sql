-- User-authored automation rules. rule_toml is the canonical
-- serialized form; everything else is denormalized for indexing.
-- activation_error captures the latest "rule failed to arm" reason
-- so broken rules show up in the dashboard.
CREATE TABLE IF NOT EXISTS rules (
    id               TEXT    PRIMARY KEY,
    name             TEXT    NOT NULL,
    description      TEXT    NOT NULL DEFAULT '',
    status           TEXT    NOT NULL DEFAULT 'enabled',
    version          INTEGER NOT NULL DEFAULT 1,
    trigger_type     TEXT    NOT NULL,
    rule_toml        TEXT    NOT NULL,
    activation_error TEXT,
    created_at       TEXT    NOT NULL,
    updated_at       TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rules_trigger_type ON rules(trigger_type);
CREATE INDEX IF NOT EXISTS idx_rules_status       ON rules(status);
