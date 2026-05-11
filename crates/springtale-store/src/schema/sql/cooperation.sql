-- Cooperation persistence — atomic CAS, environment-mediated
-- handoff, and shared mental model. See COOPERATION.md §13, §20, §21.
-- All SQL routes through springtale-store; the cooperation crate
-- talks to these tables only via the StorageBackend trait.

-- §13 Interference Detection — atomic compare-and-swap write log.
-- Implemented via SQLite BEGIN IMMEDIATE + INSERT OR REPLACE.
-- The (k) primary key + writer/tick columns supply the conflict
-- detail sled::compare_and_swap would have returned on mismatch.
CREATE TABLE IF NOT EXISTS coop_writes (
    k      TEXT    PRIMARY KEY,
    value  BLOB    NOT NULL,
    writer TEXT    NOT NULL,
    tick   INTEGER NOT NULL
) STRICT;

-- §20 Environment-Mediated Handoff — deposit/collect with TTL sweep.
-- claimed_by NULL means unclaimed; UPDATE ... RETURNING in
-- coop_collect ensures exactly-once claim.
CREATE TABLE IF NOT EXISTS coop_deposits (
    location      TEXT    PRIMARY KEY,
    payload       BLOB    NOT NULL,
    depositor     TEXT    NOT NULL,
    deposited_at  INTEGER NOT NULL,
    expires_at    INTEGER,
    claimed_by    TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS coop_deposits_exp ON coop_deposits(expires_at);

-- §21 Shared Mental Model — five tables, one per SharedMentalModel field.
CREATE TABLE IF NOT EXISTS mental_model_domain (
    formation_id    TEXT    NOT NULL,
    key             TEXT    NOT NULL,
    description     TEXT    NOT NULL,
    learned_at_unix INTEGER NOT NULL,
    confidence      REAL    NOT NULL,
    PRIMARY KEY (formation_id, key)
) STRICT;

CREATE TABLE IF NOT EXISTS mental_model_capability (
    formation_id TEXT NOT NULL,
    agent_id     TEXT NOT NULL,
    capability   TEXT NOT NULL,
    PRIMARY KEY (formation_id, agent_id, capability)
) STRICT;

CREATE TABLE IF NOT EXISTS mental_model_pattern (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    formation_id      TEXT    NOT NULL,
    trigger_text      TEXT    NOT NULL,
    participants_json TEXT    NOT NULL,
    success_count     INTEGER NOT NULL,
    failure_count     INTEGER NOT NULL,
    last_used_unix    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mm_pattern_formation
    ON mental_model_pattern(formation_id);

CREATE TABLE IF NOT EXISTS mental_model_vocabulary (
    formation_id        TEXT NOT NULL,
    term                TEXT NOT NULL,
    meaning             TEXT NOT NULL,
    established_by_json TEXT NOT NULL,
    PRIMARY KEY (formation_id, term)
) STRICT;

CREATE TABLE IF NOT EXISTS mental_model_convention (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    formation_id        TEXT    NOT NULL,
    description         TEXT    NOT NULL,
    established_by_json TEXT    NOT NULL,
    strength            REAL    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mm_convention_formation
    ON mental_model_convention(formation_id);
