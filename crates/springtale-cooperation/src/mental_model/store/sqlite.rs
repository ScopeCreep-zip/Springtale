//! SQLite-backed `Store` implementation.
//!
//! Uses `rusqlite::Connection` guarded by a `Mutex` so the `Store` trait's
//! `&self` save/load methods translate into short-held exclusive locks.
//! For single-process formations this is fine; cross-process sharing should
//! use the WAL-mode config the workspace rusqlite feature set enables.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::cadence::AgentId;
use crate::mental_model::types::SharedMentalModel;

use super::error::StoreError;
use super::rows::{
    parse_agent_id, ConventionRow, DomainRow, PatternRow, VocabularyRow,
};
use super::schema::MIGRATIONS;
use super::trait_::Store;

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// Open a connection at the given path. `:memory:` works for tests.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        Self::apply_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an in-memory connection — used by tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        Self::apply_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn apply_schema(conn: &Connection) -> Result<(), StoreError> {
        for sql in MIGRATIONS {
            conn.execute(sql, [])?;
        }
        Ok(())
    }

    fn with_conn<R>(&self, f: impl FnOnce(&mut Connection) -> Result<R, StoreError>) -> Result<R, StoreError> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|e| StoreError::InvalidRow(format!("lock poisoned: {e}")))?;
        f(&mut guard)
    }
}

impl Store for SqliteStore {
    fn save(&self, formation_id: &str, model: &SharedMentalModel) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;

            tx.execute(
                "DELETE FROM mental_model_domain WHERE formation_id = ?1",
                [formation_id],
            )?;
            for (key, entry) in &model.domain_knowledge {
                let row = DomainRow::from_entry(key, entry);
                tx.execute(
                    "INSERT INTO mental_model_domain
                        (formation_id, key, description, learned_at_unix, confidence)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![formation_id, row.key, row.description, row.learned_at_unix, row.confidence],
                )?;
            }

            tx.execute(
                "DELETE FROM mental_model_capability WHERE formation_id = ?1",
                [formation_id],
            )?;
            for (agent, caps) in &model.capability_awareness {
                for cap in caps {
                    tx.execute(
                        "INSERT INTO mental_model_capability
                            (formation_id, agent_id, capability) VALUES (?1, ?2, ?3)",
                        params![formation_id, agent.0.to_string(), cap.name],
                    )?;
                }
            }

            tx.execute(
                "DELETE FROM mental_model_pattern WHERE formation_id = ?1",
                [formation_id],
            )?;
            for pattern in &model.cooperation_patterns {
                let row = PatternRow::from_pattern(pattern)?;
                tx.execute(
                    "INSERT INTO mental_model_pattern
                        (formation_id, trigger_text, participants_json,
                         success_count, failure_count, last_used_unix)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        formation_id,
                        row.trigger_text,
                        row.participants_json,
                        row.success_count,
                        row.failure_count,
                        row.last_used_unix,
                    ],
                )?;
            }

            tx.execute(
                "DELETE FROM mental_model_vocabulary WHERE formation_id = ?1",
                [formation_id],
            )?;
            for entry in model.shared_vocabulary.values() {
                let row = VocabularyRow::from_entry(entry)?;
                tx.execute(
                    "INSERT INTO mental_model_vocabulary
                        (formation_id, term, meaning, established_by_json)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![formation_id, row.term, row.meaning, row.established_by_json],
                )?;
            }

            tx.execute(
                "DELETE FROM mental_model_convention WHERE formation_id = ?1",
                [formation_id],
            )?;
            for convention in &model.conventions {
                let row = ConventionRow::from_convention(convention)?;
                tx.execute(
                    "INSERT INTO mental_model_convention
                        (formation_id, description, established_by_json, strength)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![formation_id, row.description, row.established_by_json, row.strength],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
    }

    fn load(&self, formation_id: &str) -> Result<SharedMentalModel, StoreError> {
        self.with_conn(|conn| {
            let mut model = SharedMentalModel::default();

            let mut stmt = conn.prepare(
                "SELECT key, description, learned_at_unix, confidence
                 FROM mental_model_domain WHERE formation_id = ?1",
            )?;
            let rows = stmt
                .query_map([formation_id], |r| {
                    Ok(DomainRow {
                        key: r.get(0)?,
                        description: r.get(1)?,
                        learned_at_unix: r.get(2)?,
                        confidence: r.get(3)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for row in rows {
                let key = row.key.clone();
                model.domain_knowledge.insert(key, row.into_entry());
            }
            drop(stmt);

            let mut stmt = conn.prepare(
                "SELECT agent_id, capability FROM mental_model_capability
                 WHERE formation_id = ?1",
            )?;
            let caps = stmt
                .query_map([formation_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (agent_id_str, cap) in caps {
                let id: AgentId = parse_agent_id(&agent_id_str)?;
                model
                    .capability_awareness
                    .entry(id)
                    .or_default()
                    .push(crate::capability::CapabilityDecl::new(cap));
            }
            drop(stmt);

            let mut stmt = conn.prepare(
                "SELECT trigger_text, participants_json, success_count,
                        failure_count, last_used_unix
                 FROM mental_model_pattern WHERE formation_id = ?1",
            )?;
            let patterns = stmt
                .query_map([formation_id], |r| {
                    Ok(PatternRow {
                        trigger_text: r.get(0)?,
                        participants_json: r.get(1)?,
                        success_count: r.get(2)?,
                        failure_count: r.get(3)?,
                        last_used_unix: r.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for row in patterns {
                model.cooperation_patterns.push(row.into_pattern()?);
            }
            drop(stmt);

            let mut stmt = conn.prepare(
                "SELECT term, meaning, established_by_json
                 FROM mental_model_vocabulary WHERE formation_id = ?1",
            )?;
            let vocab = stmt
                .query_map([formation_id], |r| {
                    Ok(VocabularyRow {
                        term: r.get(0)?,
                        meaning: r.get(1)?,
                        established_by_json: r.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for row in vocab {
                let term = row.term.clone();
                model.shared_vocabulary.insert(term, row.into_entry()?);
            }
            drop(stmt);

            let mut stmt = conn.prepare(
                "SELECT description, established_by_json, strength
                 FROM mental_model_convention WHERE formation_id = ?1",
            )?;
            let conventions = stmt
                .query_map([formation_id], |r| {
                    Ok(ConventionRow {
                        description: r.get(0)?,
                        established_by_json: r.get(1)?,
                        strength: r.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for row in conventions {
                model.conventions.push(row.into_convention()?);
            }
            drop(stmt);

            Ok(model)
        })
    }

    fn clear(&self, formation_id: &str) -> Result<(), StoreError> {
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            for table in [
                "mental_model_domain",
                "mental_model_capability",
                "mental_model_pattern",
                "mental_model_vocabulary",
                "mental_model_convention",
            ] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE formation_id = ?1"),
                    [formation_id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }
}


#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::mental_model::types::{
        Convention, CooperationPattern, DomainEntry, VocabularyEntry,
    };

    fn sample_model() -> SharedMentalModel {
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();
        let mut m = SharedMentalModel::default();
        m.domain_knowledge.insert(
            "topple_window".to_owned(),
            DomainEntry {
                description: "monster topples after 3 hits to head".to_owned(),
                learned_at: Instant::now(),
                confidence: 0.8,
            },
        );
        m.capability_awareness
            .insert(agent_a, vec!["github".into(), "slack".into()]);
        m.capability_awareness
            .insert(agent_b, vec!["nostr".into()]);
        m.cooperation_patterns.push(CooperationPattern {
            trigger: "alert_fired".into(),
            participants: vec![agent_a, agent_b],
            success_count: 5,
            failure_count: 1,
            last_used: Instant::now(),
        });
        m.shared_vocabulary.insert(
            "room_7".to_owned(),
            VocabularyEntry {
                term: "room_7".to_owned(),
                meaning: "long hallway west side".to_owned(),
                established_by: vec![agent_a],
            },
        );
        m.conventions.push(Convention {
            description: "agent A handles issue creation, B handles triage".to_owned(),
            established_by: vec![agent_a, agent_b],
            strength: 0.9,
        });
        m
    }

    #[test]
    fn save_then_load_roundtrip() {
        let store = SqliteStore::open_in_memory().unwrap();
        let model = sample_model();
        store.save("f1", &model).unwrap();
        let loaded = store.load("f1").unwrap();
        assert_eq!(loaded.domain_knowledge.len(), 1);
        assert_eq!(loaded.capability_awareness.len(), 2);
        assert_eq!(loaded.cooperation_patterns.len(), 1);
        assert_eq!(loaded.shared_vocabulary.len(), 1);
        assert_eq!(loaded.conventions.len(), 1);
    }

    #[test]
    fn save_is_idempotent_upsert_like() {
        let store = SqliteStore::open_in_memory().unwrap();
        let model = sample_model();
        store.save("f1", &model).unwrap();
        store.save("f1", &model).unwrap();
        let loaded = store.load("f1").unwrap();
        assert_eq!(
            loaded.cooperation_patterns.len(),
            1,
            "second save must replace, not append"
        );
    }

    #[test]
    fn formations_are_namespaced() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.save("f1", &sample_model()).unwrap();
        let loaded = store.load("f2").unwrap();
        assert!(loaded.domain_knowledge.is_empty());
    }

    #[test]
    fn clear_removes_all_formation_rows() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.save("f1", &sample_model()).unwrap();
        store.clear("f1").unwrap();
        let loaded = store.load("f1").unwrap();
        assert!(loaded.domain_knowledge.is_empty());
        assert!(loaded.capability_awareness.is_empty());
        assert!(loaded.cooperation_patterns.is_empty());
        assert!(loaded.shared_vocabulary.is_empty());
        assert!(loaded.conventions.is_empty());
    }

    #[test]
    fn pattern_participants_round_trip() {
        let store = SqliteStore::open_in_memory().unwrap();
        let model = sample_model();
        let original_participants = model.cooperation_patterns[0].participants.clone();
        store.save("f1", &model).unwrap();
        let loaded = store.load("f1").unwrap();
        assert_eq!(
            loaded.cooperation_patterns[0].participants,
            original_participants
        );
    }
}
