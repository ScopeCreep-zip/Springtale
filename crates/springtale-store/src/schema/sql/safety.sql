-- Single-row safety configuration (`id = 1` constraint).
-- Loads before vault unlock so the app starts disguised.
-- See docs/arch/ARCHITECTURE.md §2.8 (IPV threat model).
--
-- disguise_app_name / disguise_icon_id / disguise_active /
-- panic_tap_count back the G5d app-disguise UX: the surface a
-- coerced user sees must survive process restart.
CREATE TABLE IF NOT EXISTS safety_config (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    window_title        TEXT    NOT NULL DEFAULT 'Notes',
    auto_lock_minutes   INTEGER NOT NULL DEFAULT 5,
    content_protected   INTEGER NOT NULL DEFAULT 1,
    quick_hide_shortcut TEXT    NOT NULL DEFAULT 'Ctrl+Shift+H',
    disguise_app_name   TEXT    NOT NULL DEFAULT 'Notes',
    disguise_icon_id    TEXT    NOT NULL DEFAULT 'notes',
    disguise_active     INTEGER NOT NULL DEFAULT 0,
    panic_tap_count     INTEGER NOT NULL DEFAULT 5,
    updated_at          TEXT    NOT NULL
);
