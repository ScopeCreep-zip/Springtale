-- Bot session + memory + alias tables (one logical domain).
--
-- bot_memory.content is always encrypted with the per-user key
-- derived from the vault; the schema_version column tags row
-- format so future re-encryption flows can detect old rows.

CREATE TABLE IF NOT EXISTS bot_sessions (
    user_id           TEXT NOT NULL,
    channel_id        TEXT NOT NULL,
    last_bot_message  TEXT,
    pending_command   TEXT,
    state_data        TEXT NOT NULL DEFAULT '{}',
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    PRIMARY KEY (user_id, channel_id)
);

CREATE INDEX IF NOT EXISTS idx_bot_sessions_updated ON bot_sessions(updated_at);

CREATE TABLE IF NOT EXISTS user_prefs (
    user_id               TEXT    PRIMARY KEY,
    timezone              TEXT    NOT NULL DEFAULT 'UTC',
    language              TEXT    NOT NULL DEFAULT 'en',
    notifications_enabled INTEGER NOT NULL DEFAULT 0,
    updated_at            TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS bot_memory (
    id                TEXT    PRIMARY KEY,
    user_id           TEXT    NOT NULL,
    channel_id        TEXT    NOT NULL,
    category          TEXT    NOT NULL DEFAULT 'conversation',
    schema_version    INTEGER NOT NULL DEFAULT 1,
    author            TEXT    NOT NULL DEFAULT 'user',
    source            TEXT    NOT NULL DEFAULT 'user_input',
    content_encrypted BLOB    NOT NULL,
    nonce             BLOB    NOT NULL,
    content_hash      TEXT,
    parent_id         TEXT,
    trust_score       REAL    NOT NULL DEFAULT 1.0,
    created_at        TEXT    NOT NULL,
    expires_at        TEXT
);

CREATE INDEX IF NOT EXISTS idx_bot_memory_user_channel ON bot_memory(user_id, channel_id);
CREATE INDEX IF NOT EXISTS idx_bot_memory_created      ON bot_memory(created_at);
CREATE INDEX IF NOT EXISTS idx_bot_memory_category     ON bot_memory(category);

CREATE TABLE IF NOT EXISTS bot_aliases (
    alias      TEXT PRIMARY KEY,
    target     TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL
);
