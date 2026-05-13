-- Phase A — per-rule dedupe state.
--
-- Powers `Action::Dedupe` short-circuit semantics for polling
-- recipes (rss-broadcast, page-change-watcher, calendar-feed-reminder,
-- etc.). When a chain step's dedupe key resolves to a hash that's
-- already in this table for `(formation_id, rule_id, bucket)`, the
-- dispatcher returns `ChainError::Suppressed` and the chain ends
-- with execution status `empty` (not failed).
--
-- Scoping (Phase 0.4 cooperation alignment):
--   - formation_id NULL → global rule
--   - formation_id NOT NULL → only this formation's instance of the
--     rule sees these dedupe entries
--
-- Key hashing (Phase A):
--   - Recipe author writes `${last_extract_output.entries.0.id}` etc.
--   - Runtime resolves to plaintext, hashes with blake3, inserts the
--     hex digest. Plaintext keys never touch disk (privacy: an item
--     id can reveal whose mailbox / which sender — sha-style hashing
--     keeps dedupe state PII-free per CLAUDE.md §6.10).
CREATE TABLE IF NOT EXISTS dedupe_seen (
    formation_id  TEXT,
    rule_id       TEXT    NOT NULL,
    bucket        TEXT    NOT NULL,
    key_hash      TEXT    NOT NULL,
    seen_at       INTEGER NOT NULL,           -- unix ms
    PRIMARY KEY (formation_id, rule_id, bucket, key_hash)
) STRICT;

-- LRU prune index — `ORDER BY seen_at ASC` per bucket to identify the
-- oldest entries when `history` is exceeded.
CREATE INDEX IF NOT EXISTS idx_dedupe_seen_at
    ON dedupe_seen(formation_id, rule_id, bucket, seen_at DESC);
