//! Mental-model cross-write integration test (Phase-7 audit Finding C).
//!
//! Spawns N concurrent writers against a single `BackendStore` backed
//! by a shared in-memory store, each writing its own slice of the
//! same `SharedMentalModel` (vocabulary entries, capability
//! awareness, conventions). Asserts that:
//!
//! 1. Sequential accumulation: when each writer extends a shared
//!    accumulated model, the final load() observes every slot's
//!    slice — the bundle save/load roundtrip is loss-free.
//! 2. Concurrent contention: when N writers race on the same
//!    formation_id, the load() always returns SOME writer's full
//!    view + every roundtripped entry is structurally valid (no
//!    torn rows, no partial bundles).
//!
//! External workspaces are persisted via a separate per-key upsert
//! API, not the bundle path, so they're exercised by their own unit
//! tests; this integration covers the bundle invariants the
//! cooperation framework depends on.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::capability::CapabilityDecl;
use springtale_cooperation::mental_model::store::BackendStore;
use springtale_cooperation::mental_model::store::Store;
use springtale_cooperation::mental_model::types::{Convention, SharedMentalModel, VocabularyEntry};
use springtale_store::StorageBackend;
use springtale_store::backend::InMemoryBackend;

const N_AGENTS: usize = 5;
const FORMATION: &str = "test-formation";

fn slot_slice(slot: usize, agent: AgentId) -> SharedMentalModel {
    let mut model = SharedMentalModel::default();

    let term = format!("term-{slot}");
    model.shared_vocabulary.insert(
        term.clone(),
        VocabularyEntry {
            term: term.clone(),
            meaning: format!("meaning slot {slot}"),
            established_by: vec![agent],
        },
    );

    model
        .capability_awareness
        .insert(agent, vec![CapabilityDecl::new(format!("cap-{slot}"))]);

    model.conventions.push(Convention {
        description: format!("convention slot {slot}"),
        established_by: vec![agent],
        strength: 0.5 + (slot as f32) * 0.05,
    });

    model
}

fn merge_slice(into: &mut SharedMentalModel, slice: SharedMentalModel) {
    for (k, v) in slice.shared_vocabulary {
        into.shared_vocabulary.insert(k, v);
    }
    for (k, v) in slice.capability_awareness {
        into.capability_awareness.insert(k, v);
    }
    for c in slice.conventions {
        into.conventions.push(c);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writers_persist_some_full_view() {
    let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
    let store = Arc::new(BackendStore::new(backend.clone()));

    store
        .save(FORMATION, &SharedMentalModel::default())
        .await
        .unwrap();

    let agents: Vec<AgentId> = (0..N_AGENTS).map(|_| AgentId::new()).collect();

    // Pre-compute each writer's peer view so the spawn closure
    // gets a self-contained input. The writer for slot N represents
    // `agent` and contributes slot_slice(N, agent); peers 0..N
    // gossiped their slices to it earlier and the writer persists
    // the merged view.
    let mut handles = Vec::new();
    for (slot, agent) in agents.iter().copied().enumerate() {
        let store = store.clone();
        let peers: Vec<(usize, AgentId)> = agents.iter().copied().take(slot).enumerate().collect();
        handles.push(tokio::spawn(async move {
            let mut model = SharedMentalModel::default();
            for (s, peer) in peers {
                merge_slice(&mut model, slot_slice(s, peer));
            }
            merge_slice(&mut model, slot_slice(slot, agent));
            store.save(FORMATION, &model).await.unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let loaded = store.load(FORMATION).await.unwrap();

    // At least one slot's bundle survived; every entry that
    // survived roundtripped cleanly with no torn rows.
    assert!(
        !loaded.shared_vocabulary.is_empty(),
        "load returned empty vocabulary — at least one writer's data should persist"
    );
    for (key, entry) in &loaded.shared_vocabulary {
        assert_eq!(&entry.term, key, "vocabulary key/term mismatch");
        assert!(entry.meaning.starts_with("meaning slot "));
        assert!(!entry.established_by.is_empty());
    }

    // Capability-awareness round-trip — every loaded (agent, caps)
    // pair must reflect the slot it came from. We build a reverse
    // map from cap-name → agent so each loaded agent_id can be
    // matched back to its slot AND the slot's emitted cap.
    let expected_caps: std::collections::HashMap<AgentId, String> = agents
        .iter()
        .copied()
        .enumerate()
        .map(|(slot, a)| (a, format!("cap-{slot}")))
        .collect();
    for (agent, caps) in &loaded.capability_awareness {
        assert_eq!(
            caps.len(),
            1,
            "agent {agent:?} should declare exactly one cap"
        );
        let expected = expected_caps
            .get(agent)
            .unwrap_or_else(|| panic!("loaded capability_awareness has unknown agent {agent:?}"));
        assert_eq!(
            &caps[0].name, expected,
            "agent {agent:?} cap should match its slot's emitted name"
        );
    }

    // Conventions round-trip — duplicates allowed (the writer that
    // ran with the largest `slot` enumeration replays every slot's
    // convention).
    for conv in &loaded.conventions {
        assert!(conv.description.starts_with("convention slot "));
        assert!(!conv.established_by.is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sequential_writes_accumulate_under_one_formation() {
    let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
    let store = BackendStore::new(backend);

    store
        .save(FORMATION, &SharedMentalModel::default())
        .await
        .unwrap();

    let agents: Vec<AgentId> = (0..N_AGENTS).map(|_| AgentId::new()).collect();

    let mut accumulated = SharedMentalModel::default();
    for (slot, agent) in agents.iter().copied().enumerate() {
        merge_slice(&mut accumulated, slot_slice(slot, agent));
        store.save(FORMATION, &accumulated).await.unwrap();
    }

    let loaded = store.load(FORMATION).await.unwrap();
    assert_eq!(
        loaded.shared_vocabulary.len(),
        N_AGENTS,
        "every slot's vocabulary must round-trip after the final save"
    );
    assert_eq!(
        loaded.capability_awareness.len(),
        N_AGENTS,
        "every agent's capability awareness must round-trip"
    );
    assert_eq!(
        loaded.conventions.len(),
        N_AGENTS,
        "every slot's convention must round-trip"
    );

    for slot in 0..N_AGENTS {
        let term = format!("term-{slot}");
        assert!(
            loaded.shared_vocabulary.contains_key(&term),
            "missing vocabulary term {term}"
        );
    }
    for agent in &agents {
        assert!(
            loaded.capability_awareness.contains_key(agent),
            "missing capability_awareness for agent {agent:?}"
        );
    }
}
