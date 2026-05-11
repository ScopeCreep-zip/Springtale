-- Installed first-party + WASM connectors.
-- manifest_json is the connector's manifest (declared triggers,
-- actions, capabilities, signature). enabled flips a connector off
-- without uninstalling it.
CREATE TABLE IF NOT EXISTS connectors (
    name          TEXT    PRIMARY KEY,
    version       TEXT    NOT NULL,
    author        TEXT    NOT NULL,
    description   TEXT    NOT NULL DEFAULT '',
    manifest_json TEXT    NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    installed_at  TEXT    NOT NULL
);
