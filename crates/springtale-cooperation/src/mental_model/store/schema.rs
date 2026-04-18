//! Schema — idempotent CREATE IF NOT EXISTS statements applied on open.
//!
//! Five tables, one per `SharedMentalModel` field (domain_knowledge,
//! capability_awareness, cooperation_patterns, shared_vocabulary,
//! conventions). Composite-key primary keys so repeated writes for the same
//! (formation_id, key) upsert cleanly.

pub const MIGRATIONS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS mental_model_domain (
        formation_id TEXT NOT NULL,
        key TEXT NOT NULL,
        description TEXT NOT NULL,
        learned_at_unix INTEGER NOT NULL,
        confidence REAL NOT NULL,
        PRIMARY KEY (formation_id, key)
    )",
    "CREATE TABLE IF NOT EXISTS mental_model_capability (
        formation_id TEXT NOT NULL,
        agent_id TEXT NOT NULL,
        capability TEXT NOT NULL,
        PRIMARY KEY (formation_id, agent_id, capability)
    )",
    "CREATE TABLE IF NOT EXISTS mental_model_pattern (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        formation_id TEXT NOT NULL,
        trigger_text TEXT NOT NULL,
        participants_json TEXT NOT NULL,
        success_count INTEGER NOT NULL,
        failure_count INTEGER NOT NULL,
        last_used_unix INTEGER NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_mm_pattern_formation
        ON mental_model_pattern(formation_id)",
    "CREATE TABLE IF NOT EXISTS mental_model_vocabulary (
        formation_id TEXT NOT NULL,
        term TEXT NOT NULL,
        meaning TEXT NOT NULL,
        established_by_json TEXT NOT NULL,
        PRIMARY KEY (formation_id, term)
    )",
    "CREATE TABLE IF NOT EXISTS mental_model_convention (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        formation_id TEXT NOT NULL,
        description TEXT NOT NULL,
        established_by_json TEXT NOT NULL,
        strength REAL NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_mm_convention_formation
        ON mental_model_convention(formation_id)",
];
