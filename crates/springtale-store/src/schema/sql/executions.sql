-- Phase B — per-fire executions log with privacy defaults.
--
-- ## Why a new table
--
-- The existing tables are orthogonal and cover different signals:
--   - `events`            → trigger metadata (Cron fired at T, Webhook arrived)
--   - `execution_results` → per-action output payload (legacy; per-connector cap)
--   - `audit_trail`       → Sentinel verdicts (allow / deny / panic-wipe)
--
-- `executions` records the **chain lifecycle**: one row per
-- ChainContext fire, with status / momentum / cooperation scope /
-- error-kind. `execution_steps` records each step inside that
-- chain. Together they answer "which agent ran which step in
-- which formation, and what happened?" — the cooperation-aligned
-- observability the v2 plan calls for.
--
-- ## Privacy defaults (stricter than Apify and n8n)
--
-- - Sizes only by default (input_bytes / output_bytes). NEVER content.
-- - `error_kind` is an enum tag string; full messages stay in `tracing` logs.
-- - 14-day retention via `retention_until`; configurable per-bot 1d–90d.
-- - Opt-in content retention via `input_blob_ref` / `output_blob_ref`
--   pointing at a separate KV store (Phase C — `bot.retain_step_content`).
-- - `panic_wipe` drops both tables.
--
-- ## Identifiers
--
-- - `id` is a ULID — lexicographically sortable by time, generated
--   in the dispatcher. Cheaper joins than autoincrement; safer
--   correlation between processes than UUID.
-- - `retry_of` references a previous `executions.id` when a chain
--   was retried (Phase C); NULL for the original fire.

CREATE TABLE IF NOT EXISTS executions (
    id              TEXT    PRIMARY KEY,           -- ULID, lex-sortable
    bot_id          TEXT,                          -- nullable for global rules
    formation_id    TEXT,                          -- nullable for solo agents
    rule_id         TEXT,
    recipe_id       TEXT,
    started_at      INTEGER NOT NULL,              -- unix ms
    finished_at     INTEGER,                       -- unix ms; NULL while running
    mode            TEXT    NOT NULL,              -- cron|webhook|connector_event|file_watch|manual|cooperation|retry|dry_run
    status          TEXT    NOT NULL,              -- running|succeeded|failed|empty|aborted|timed_out
    momentum        TEXT,                          -- cold|warming|hot|fever
    trigger_summary TEXT,                          -- short text (e.g. "Cron 0 7 * * *")
    error_kind      TEXT,                          -- enum-typed tag, NEVER message
    duration_ms     INTEGER,                       -- denormalized: finished_at - started_at
    retention_until INTEGER NOT NULL,              -- unix ms; vacuum_executions purges past
    retry_of        TEXT REFERENCES executions(id)
) STRICT;

-- Query patterns: agent timeline / formation timeline / status filter /
-- retention sweep. Each gets a dedicated index so the executions
-- panel paginates cleanly even at scale.
CREATE INDEX IF NOT EXISTS idx_executions_bot
    ON executions(bot_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_executions_formation
    ON executions(formation_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_executions_status
    ON executions(status, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_executions_retention
    ON executions(retention_until);

CREATE TABLE IF NOT EXISTS execution_steps (
    execution_id    TEXT    NOT NULL REFERENCES executions(id) ON DELETE CASCADE,
    step_index      INTEGER NOT NULL,
    step_kind       TEXT    NOT NULL,              -- run_connector|ai_complete|extract|dedupe|send_message|...
    connector       TEXT,
    action          TEXT,
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    status          TEXT    NOT NULL,              -- succeeded|failed|suppressed|skipped
    input_bytes     INTEGER NOT NULL DEFAULT 0,    -- size only, NEVER content
    output_bytes    INTEGER NOT NULL DEFAULT 0,
    output_kind     TEXT,                          -- json|html|text|binary
    error_kind      TEXT,                          -- enum-typed tag
    -- Opt-in content retention (NULL by default; populated only when
    -- the bot's `retain_step_content` setting is true). Phase C uses
    -- the blob refs to fetch content from a separate KV store —
    -- inline content would defeat the privacy default.
    input_blob_ref  TEXT,
    output_blob_ref TEXT,
    PRIMARY KEY (execution_id, step_index)
) STRICT;
