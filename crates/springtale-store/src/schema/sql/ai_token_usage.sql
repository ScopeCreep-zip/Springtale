-- Per-bot AI token usage (OWASP LLM10 mitigation).
--
-- The runtime's `SqliteTokenQuota` reads + writes this table during
-- every AI call: `check_and_reserve` UPSERTs the per-day row with a
-- pessimistic upper bound; `commit` adjusts the row to the actual
-- token count. The compound primary key (agent_id, day_ymd) gives the
-- backend a single deterministic row per bot per UTC day so reservation
-- + commit race correctness is just a SQLite UPSERT with arithmetic.
--
-- `day_ymd` is the integer YYYY*1000 + ordinal-day-of-year used by the
-- in-process quota — same packing keeps the SQLite-backed and
-- in-process backends interchangeable.
CREATE TABLE IF NOT EXISTS ai_token_usage (
    agent_id     TEXT    NOT NULL,
    day_ymd      INTEGER NOT NULL,
    tokens_used  INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT    NOT NULL,
    PRIMARY KEY (agent_id, day_ymd)
);

CREATE INDEX IF NOT EXISTS idx_ai_token_usage_day ON ai_token_usage(day_ymd);
