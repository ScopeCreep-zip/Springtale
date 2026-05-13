-- User-authored automation rules. rule_toml is the canonical
-- serialized form; everything else is denormalized for indexing.
-- activation_error captures the latest "rule failed to arm" reason
-- so broken rules show up in the dashboard.
--
-- Phase 0.4: cooperation scoping via the `owner_kind` /
-- `owner_agent_id` / `owner_formation_id` columns. Mirrors the
-- `RuleOwner` enum in `springtale-core::rule::types`. The TOML
-- payload still carries the owner field — these columns are the
-- denormalized index, so listings like "rules owned by formation X"
-- don't have to parse every row's TOML. The engine's match path uses
-- the Rule's deserialized owner; these columns serve admin queries
-- and (Phase B+) executions log joins.
CREATE TABLE IF NOT EXISTS rules (
    id                  TEXT    PRIMARY KEY,
    name                TEXT    NOT NULL,
    description         TEXT    NOT NULL DEFAULT '',
    status              TEXT    NOT NULL DEFAULT 'enabled',
    version             INTEGER NOT NULL DEFAULT 1,
    trigger_type        TEXT    NOT NULL,
    rule_toml           TEXT    NOT NULL,
    activation_error    TEXT,
    -- 'global' | 'agent' | 'formation' — matches RuleOwner serde tag.
    owner_kind          TEXT    NOT NULL DEFAULT 'global',
    -- UUID string when owner_kind = 'agent', else NULL.
    owner_agent_id      TEXT,
    -- UUID string when owner_kind = 'formation', else NULL.
    owner_formation_id  TEXT,
    created_at          TEXT    NOT NULL,
    updated_at          TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rules_trigger_type      ON rules(trigger_type);
CREATE INDEX IF NOT EXISTS idx_rules_status            ON rules(status);
CREATE INDEX IF NOT EXISTS idx_rules_owner_kind        ON rules(owner_kind);
CREATE INDEX IF NOT EXISTS idx_rules_owner_agent       ON rules(owner_agent_id);
CREATE INDEX IF NOT EXISTS idx_rules_owner_formation   ON rules(owner_formation_id);
