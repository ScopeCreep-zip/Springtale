-- Safety configuration — persisted outside the vault.
-- Single-row table (id=1 constraint) — loads before vault unlock
-- so the app starts disguised.
--
-- Per ARCHITECTURE.md §2.8 (IPV threat model):
-- - window_title defaults to "Notes" (disguise-first)
-- - auto_lock_minutes defaults to 5 (vault locks after inactivity)
-- - content_protected defaults to enabled (no screenshots)
-- - quick_hide_shortcut configurable system-wide hotkey

CREATE TABLE IF NOT EXISTS safety_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    window_title TEXT NOT NULL DEFAULT 'Notes',
    auto_lock_minutes INTEGER NOT NULL DEFAULT 5,
    content_protected INTEGER NOT NULL DEFAULT 1,
    quick_hide_shortcut TEXT NOT NULL DEFAULT 'Ctrl+Shift+H',
    updated_at TEXT NOT NULL
);
