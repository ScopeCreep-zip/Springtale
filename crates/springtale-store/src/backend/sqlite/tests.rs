#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::backend::trait_::StorageBackend;
use springtale_core::rule::action::Action;
use springtale_core::rule::trigger::Trigger;
use springtale_core::rule::types::{RuleOwner, RuleStatus, RuleVersion};

fn test_rule(name: &str) -> Rule {
    Rule {
        id: RuleId::new(),
        name: name.into(),
        description: "test rule".into(),
        status: RuleStatus::Enabled,
        version: RuleVersion(1),
        trigger: Trigger::Cron {
            expression: "0 9 * * *".into(),
        },
        conditions: vec![],
        actions: vec![Action::SendMessage {
            text: "hello".into(),
        }],
        owner: RuleOwner::Global,
    }
}

#[tokio::test]
async fn test_insert_and_list_rules() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let rule = test_rule("test-rule");
    let id = store.insert_rule(&rule).await.unwrap();

    let rules = store.list_rules().await.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "test-rule");
    assert_eq!(rules[0].id, id);
}

#[tokio::test]
async fn test_find_rules_by_trigger() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store.insert_rule(&test_rule("cron-rule")).await.unwrap();

    let found = store.find_rules_by_trigger("Cron").await.unwrap();
    assert_eq!(found.len(), 1);

    let not_found = store.find_rules_by_trigger("FileWatch").await.unwrap();
    assert!(not_found.is_empty());
}

#[tokio::test]
async fn test_toggle_rule() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let rule = test_rule("toggle-rule");
    let id = store.insert_rule(&rule).await.unwrap();

    store.toggle_rule(&id, false).await.unwrap();
    let rules = store.find_rules_by_trigger("Cron").await.unwrap();
    assert!(rules.is_empty()); // disabled rules not returned by find_by_trigger
}

#[tokio::test]
async fn test_delete_rule() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let rule = test_rule("delete-rule");
    let id = store.insert_rule(&rule).await.unwrap();

    store.delete_rule(&id).await.unwrap();
    let rules = store.list_rules().await.unwrap();
    assert!(rules.is_empty());
}

#[tokio::test]
async fn test_register_and_list_connectors() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let row = ConnectorRow {
        name: "connector-test".into(),
        version: "1.0.0".into(),
        author: "test".into(),
        description: "a test connector".into(),
        manifest_json: r#"{"name":"connector-test"}"#.into(),
        enabled: true,
        installed_at: Utc::now(),
    };
    store.register_connector(&row).await.unwrap();

    let connectors = store.list_connectors().await.unwrap();
    assert_eq!(connectors.len(), 1);
    assert_eq!(connectors[0].name, "connector-test");
    assert!(connectors[0].enabled);
}

#[tokio::test]
async fn test_set_connector_enabled() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let row = ConnectorRow {
        name: "connector-toggle".into(),
        version: "1.0.0".into(),
        author: "test".into(),
        description: String::new(),
        manifest_json: "{}".into(),
        enabled: true,
        installed_at: Utc::now(),
    };
    store.register_connector(&row).await.unwrap();
    store
        .set_connector_enabled("connector-toggle", false)
        .await
        .unwrap();

    let connectors = store.list_connectors().await.unwrap();
    assert!(!connectors[0].enabled);
}

#[tokio::test]
async fn test_log_and_list_events() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let event = EventEntry {
        id: uuid::Uuid::new_v4(),
        connector_name: "connector-kick".into(),
        trigger_type: "ConnectorEvent".into(),
        timestamp: Utc::now(),
        action_taken: "sent message".into(),
    };
    store.log_event(&event).await.unwrap();

    let events = store
        .list_events(&EventFilter {
            connector_name: Some("connector-kick".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].action_taken, "sent message");
}

#[tokio::test]
async fn test_delete_events_before() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let old_event = EventEntry {
        id: uuid::Uuid::new_v4(),
        connector_name: "test".into(),
        trigger_type: "Cron".into(),
        timestamp: Utc::now() - chrono::Duration::days(30),
        action_taken: "old".into(),
    };
    let new_event = EventEntry {
        id: uuid::Uuid::new_v4(),
        connector_name: "test".into(),
        trigger_type: "Cron".into(),
        timestamp: Utc::now(),
        action_taken: "new".into(),
    };
    store.log_event(&old_event).await.unwrap();
    store.log_event(&new_event).await.unwrap();

    let deleted = store
        .delete_events_before(&(Utc::now() - chrono::Duration::days(7)))
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    let remaining = store.list_events(&EventFilter::default()).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].action_taken, "new");
}

#[tokio::test]
async fn test_enqueue_and_dequeue_job() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let job = JobRow {
        id: JobId::new(),
        payload: serde_json::json!({"action": "test"}),
        status: "pending".into(),
        attempts: 0,
        max_attempts: 3,
        created_at: Utc::now(),
        started_at: None,
        last_error: None,
    };
    store.enqueue_job(&job).await.unwrap();

    let dequeued = store.dequeue_job().await.unwrap();
    assert!(dequeued.is_some());
    let dequeued = dequeued.unwrap();
    assert_eq!(dequeued.status, "running");
    assert_eq!(dequeued.attempts, 1);
}

#[tokio::test]
async fn test_complete_job() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let job = JobRow {
        id: JobId::new(),
        payload: serde_json::json!({}),
        status: "pending".into(),
        attempts: 0,
        max_attempts: 1,
        created_at: Utc::now(),
        started_at: None,
        last_error: None,
    };
    let id = store.enqueue_job(&job).await.unwrap();
    store.dequeue_job().await.unwrap();

    store.complete_job(&id).await.unwrap();

    // No more pending jobs
    let next = store.dequeue_job().await.unwrap();
    assert!(next.is_none());
}

#[tokio::test]
async fn test_fail_job() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let job = JobRow {
        id: JobId::new(),
        payload: serde_json::json!({}),
        status: "pending".into(),
        attempts: 0,
        max_attempts: 1,
        created_at: Utc::now(),
        started_at: None,
        last_error: None,
    };
    let id = store.enqueue_job(&job).await.unwrap();
    store.dequeue_job().await.unwrap();

    store.fail_job(&id, "something broke").await.unwrap();

    let next = store.dequeue_job().await.unwrap();
    assert!(next.is_none());
}

#[tokio::test]
async fn test_dequeue_empty_returns_none() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let result = store.dequeue_job().await.unwrap();
    assert!(result.is_none());
}

// ── Bot Sessions ──────────────────────────────────────────

fn test_session(user: &str, channel: &str) -> crate::schema::bot::SessionRow {
    crate::schema::bot::SessionRow {
        user_id: user.into(),
        channel_id: channel.into(),
        last_bot_message: None,
        pending_command: None,
        state_data: "{}".into(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[tokio::test]
async fn test_upsert_and_get_session() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let session = test_session("user1", "chan1");
    store.upsert_session(&session).await.unwrap();

    let loaded = store.get_session("user1", "chan1").await.unwrap();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.user_id, "user1");
    assert_eq!(loaded.channel_id, "chan1");
}

#[tokio::test]
async fn test_session_upsert_updates_existing() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let mut session = test_session("user1", "chan1");
    store.upsert_session(&session).await.unwrap();

    session.pending_command = Some("search".into());
    session.updated_at = Utc::now();
    store.upsert_session(&session).await.unwrap();

    let loaded = store.get_session("user1", "chan1").await.unwrap().unwrap();
    assert_eq!(loaded.pending_command.as_deref(), Some("search"));
}

#[tokio::test]
async fn test_get_session_not_found() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let result = store.get_session("nobody", "nowhere").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_delete_session() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store
        .upsert_session(&test_session("u1", "c1"))
        .await
        .unwrap();
    store.delete_session("u1", "c1").await.unwrap();
    assert!(store.get_session("u1", "c1").await.unwrap().is_none());
}

#[tokio::test]
async fn test_session_isolation() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let mut s1 = test_session("user1", "chan1");
    s1.pending_command = Some("cmd1".into());
    let mut s2 = test_session("user2", "chan1");
    s2.pending_command = Some("cmd2".into());

    store.upsert_session(&s1).await.unwrap();
    store.upsert_session(&s2).await.unwrap();

    let loaded1 = store.get_session("user1", "chan1").await.unwrap().unwrap();
    let loaded2 = store.get_session("user2", "chan1").await.unwrap().unwrap();
    assert_eq!(loaded1.pending_command.as_deref(), Some("cmd1"));
    assert_eq!(loaded2.pending_command.as_deref(), Some("cmd2"));
}

// ── User Preferences ──────────────────────────────────────

#[tokio::test]
async fn test_upsert_and_get_user_prefs() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let prefs = crate::schema::bot::UserPrefsRow {
        user_id: "user1".into(),
        timezone: "America/New_York".into(),
        language: "en".into(),
        notifications_enabled: false,
        updated_at: Utc::now(),
    };
    store.upsert_user_prefs(&prefs).await.unwrap();

    let loaded = store.get_user_prefs("user1").await.unwrap().unwrap();
    assert_eq!(loaded.timezone, "America/New_York");
    assert!(!loaded.notifications_enabled);
}

#[tokio::test]
async fn test_user_prefs_not_found() {
    let store = SqliteBackend::open_in_memory().unwrap();
    assert!(store.get_user_prefs("nobody").await.unwrap().is_none());
}

#[tokio::test]
async fn test_user_prefs_upsert_updates() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let prefs = crate::schema::bot::UserPrefsRow {
        user_id: "u1".into(),
        timezone: "UTC".into(),
        language: "en".into(),
        notifications_enabled: false,
        updated_at: Utc::now(),
    };
    store.upsert_user_prefs(&prefs).await.unwrap();

    let updated = crate::schema::bot::UserPrefsRow {
        timezone: "Europe/London".into(),
        notifications_enabled: true,
        updated_at: Utc::now(),
        ..prefs
    };
    store.upsert_user_prefs(&updated).await.unwrap();

    let loaded = store.get_user_prefs("u1").await.unwrap().unwrap();
    assert_eq!(loaded.timezone, "Europe/London");
    assert!(loaded.notifications_enabled);
}

// ── Bot Memory ────────────────────────────────────────────

fn test_memory(user: &str, channel: &str, content: &[u8]) -> crate::schema::bot::MemoryRow {
    crate::schema::bot::MemoryRow {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.into(),
        channel_id: channel.into(),
        category: "conversation".into(),
        schema_version: 1,
        author: "user".into(),
        source: "user_input".into(),
        content_encrypted: content.to_vec(),
        nonce: vec![0u8; 24],
        content_hash: None,
        parent_id: None,
        trust_score: 1.0,
        created_at: Utc::now(),
        expires_at: None,
    }
}

#[tokio::test]
async fn test_insert_and_get_memory() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let entry = test_memory("u1", "c1", b"encrypted_data");
    store.insert_memory(&entry).await.unwrap();

    let entries = store.get_memory("u1", "c1", 10).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].content_encrypted, b"encrypted_data");
    assert_eq!(entries[0].author, "user");
    assert!((entries[0].trust_score - 1.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_get_memory_respects_limit() {
    let store = SqliteBackend::open_in_memory().unwrap();
    for i in 0..5 {
        let mut entry = test_memory("u1", "c1", format!("msg{i}").as_bytes());
        entry.created_at = Utc::now() + chrono::Duration::seconds(i);
        store.insert_memory(&entry).await.unwrap();
    }

    let entries = store.get_memory("u1", "c1", 3).await.unwrap();
    assert_eq!(entries.len(), 3);
}

#[tokio::test]
async fn test_delete_memory() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store
        .insert_memory(&test_memory("u1", "c1", b"a"))
        .await
        .unwrap();
    store
        .insert_memory(&test_memory("u1", "c1", b"b"))
        .await
        .unwrap();

    let deleted = store.delete_memory("u1", "c1").await.unwrap();
    assert_eq!(deleted, 2);
    assert!(store.get_memory("u1", "c1", 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn test_compact_memory() {
    let store = SqliteBackend::open_in_memory().unwrap();
    for i in 0..10 {
        let mut entry = test_memory("u1", "c1", format!("msg{i}").as_bytes());
        entry.created_at = Utc::now() + chrono::Duration::seconds(i);
        store.insert_memory(&entry).await.unwrap();
    }

    let deleted = store.compact_memory("u1", "c1", 3).await.unwrap();
    assert_eq!(deleted, 7);

    let remaining = store.get_memory("u1", "c1", 100).await.unwrap();
    assert_eq!(remaining.len(), 3);
}

#[tokio::test]
async fn test_memory_isolation_across_users() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store
        .insert_memory(&test_memory("u1", "c1", b"user1_data"))
        .await
        .unwrap();
    store
        .insert_memory(&test_memory("u2", "c1", b"user2_data"))
        .await
        .unwrap();

    let u1_entries = store.get_memory("u1", "c1", 10).await.unwrap();
    assert_eq!(u1_entries.len(), 1);
    assert_eq!(u1_entries[0].content_encrypted, b"user1_data");
}

// ── Bot Aliases ───────────────────────────────────────────

#[tokio::test]
async fn test_upsert_and_list_aliases() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store.upsert_alias("s", "search", "user1").await.unwrap();
    store.upsert_alias("g", "github", "user1").await.unwrap();

    let aliases = store.list_aliases().await.unwrap();
    assert_eq!(aliases.len(), 2);
    assert_eq!(aliases[0], ("g".into(), "github".into()));
    assert_eq!(aliases[1], ("s".into(), "search".into()));
}

#[tokio::test]
async fn test_upsert_alias_updates_existing() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store.upsert_alias("s", "search", "user1").await.unwrap();
    store.upsert_alias("s", "status", "user2").await.unwrap();

    let aliases = store.list_aliases().await.unwrap();
    assert_eq!(aliases.len(), 1);
    assert_eq!(aliases[0], ("s".into(), "status".into()));
}

#[tokio::test]
async fn test_delete_alias() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store.upsert_alias("s", "search", "user1").await.unwrap();
    store.delete_alias("s").await.unwrap();
    assert!(store.list_aliases().await.unwrap().is_empty());
}

// ── Audit Trail ──────────────────────────────────────────

fn test_audit_entry(connector: &str, verdict: &str) -> crate::schema::audit::AuditEntry {
    // Chain columns (prev_hash / row_hash / chain_seq) are populated
    // by the repository on INSERT; callers leave them empty.
    crate::schema::audit::AuditEntry {
        id: uuid::Uuid::new_v4(),
        timestamp: Utc::now(),
        connector_name: connector.into(),
        action_type: "RunConnector".into(),
        action_summary: "test action".into(),
        verdict: verdict.into(),
        verdict_reason: String::new(),
        result: "ok".into(),
        prev_hash: String::new(),
        row_hash: String::new(),
        chain_seq: 0,
    }
}

#[tokio::test]
async fn test_insert_and_list_audit_entries() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store
        .insert_audit_entry(&test_audit_entry("connector-test", "go"))
        .await
        .unwrap();
    store
        .insert_audit_entry(&test_audit_entry("connector-test", "throttle"))
        .await
        .unwrap();

    let entries = store
        .list_audit_entries(&crate::schema::audit::AuditFilter::default())
        .await
        .unwrap();
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn test_audit_filter_by_connector() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store
        .insert_audit_entry(&test_audit_entry("connector-a", "go"))
        .await
        .unwrap();
    store
        .insert_audit_entry(&test_audit_entry("connector-b", "go"))
        .await
        .unwrap();

    let entries = store
        .list_audit_entries(&crate::schema::audit::AuditFilter {
            connector_name: Some("connector-a".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].connector_name, "connector-a");
}

#[tokio::test]
async fn test_audit_filter_by_verdict() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store
        .insert_audit_entry(&test_audit_entry("test", "go"))
        .await
        .unwrap();
    store
        .insert_audit_entry(&test_audit_entry("test", "throttle"))
        .await
        .unwrap();
    store
        .insert_audit_entry(&test_audit_entry("test", "pause"))
        .await
        .unwrap();

    let entries = store
        .list_audit_entries(&crate::schema::audit::AuditFilter {
            verdict: Some("throttle".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].verdict, "throttle");
}

#[tokio::test]
async fn test_audit_delete_before() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let mut old = test_audit_entry("test", "go");
    old.timestamp = Utc::now() - chrono::Duration::days(30);
    store.insert_audit_entry(&old).await.unwrap();

    let new = test_audit_entry("test", "go");
    store.insert_audit_entry(&new).await.unwrap();

    let deleted = store
        .delete_audit_before(&(Utc::now() - chrono::Duration::days(7)))
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    let remaining = store
        .list_audit_entries(&crate::schema::audit::AuditFilter::default())
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
}

#[tokio::test]
async fn test_audit_export_time_range() {
    let store = SqliteBackend::open_in_memory().unwrap();
    let entry = test_audit_entry("test", "go");
    store.insert_audit_entry(&entry).await.unwrap();

    let entries = store
        .export_audit(
            &(Utc::now() - chrono::Duration::hours(1)),
            &(Utc::now() + chrono::Duration::hours(1)),
        )
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
}

// ── Cooperation: CAS + deposits + mental model ──────────────────────

#[tokio::test]
async fn coop_cas_write_first_applies() {
    use crate::schema::cooperation::CoopCasOutcome;
    let store = SqliteBackend::open_in_memory().unwrap();
    let outcome = store
        .coop_cas_write(1, "writer-a", "k", None, b"v1")
        .await
        .unwrap();
    assert!(matches!(outcome, CoopCasOutcome::Applied));
}

#[tokio::test]
async fn coop_cas_write_mismatch_surfaces_current() {
    use crate::schema::cooperation::CoopCasOutcome;
    let store = SqliteBackend::open_in_memory().unwrap();
    store
        .coop_cas_write(1, "writer-a", "k", None, b"v1")
        .await
        .unwrap();
    let outcome = store
        .coop_cas_write(2, "writer-b", "k", None, b"v2")
        .await
        .unwrap();
    match outcome {
        CoopCasOutcome::Mismatch {
            current_value,
            current_writer,
            ..
        } => {
            assert_eq!(current_value, b"v1");
            assert_eq!(current_writer, "writer-a");
        }
        _ => panic!("expected Mismatch"),
    }
}

#[tokio::test]
async fn coop_deposit_collect_roundtrip() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store
        .coop_deposit("loc", b"payload", "dep-1", None)
        .await
        .unwrap();
    let got = store.coop_collect("loc", "col-1").await.unwrap();
    assert_eq!(got.as_deref(), Some(b"payload".as_slice()));
    // Second collect on the same location must fail — exactly-once.
    let second = store.coop_collect("loc", "col-2").await.unwrap();
    assert!(second.is_none());
}

#[tokio::test]
async fn coop_list_deposits_returns_all() {
    let store = SqliteBackend::open_in_memory().unwrap();
    store.coop_deposit("a", b"1", "dep", None).await.unwrap();
    store.coop_deposit("b", b"2", "dep", None).await.unwrap();
    let rows = store.coop_list_deposits().await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r.location == "a"));
    assert!(rows.iter().any(|r| r.location == "b"));
}

#[tokio::test]
async fn coop_sweep_removes_expired() {
    let store = SqliteBackend::open_in_memory().unwrap();
    // TTL of 0 seconds → expires_at == now, immediately stale.
    store
        .coop_deposit("stale", b"x", "dep", Some(0))
        .await
        .unwrap();
    // Let the wall clock advance past the deposit's expires_at.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let swept = store.coop_sweep_expired().await.unwrap();
    assert_eq!(swept, 1);
    let rows = store.coop_list_deposits().await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn mental_model_save_load_roundtrip() {
    use crate::schema::mental_model::{MentalModelBundle, MentalModelDomainRow};
    let store = SqliteBackend::open_in_memory().unwrap();
    let bundle = MentalModelBundle {
        domain: vec![MentalModelDomainRow {
            key: "topple_window".to_owned(),
            description: "3 hits to head".to_owned(),
            learned_at_unix: 1_000_000,
            confidence: 0.8,
        }],
        ..Default::default()
    };
    store.mental_model_save("f1", &bundle).await.unwrap();
    let loaded = store.mental_model_load("f1").await.unwrap();
    assert_eq!(loaded.domain.len(), 1);
    assert_eq!(loaded.domain[0].key, "topple_window");
}

#[tokio::test]
async fn mental_model_clear_removes_rows() {
    use crate::schema::mental_model::{MentalModelBundle, MentalModelDomainRow};
    let store = SqliteBackend::open_in_memory().unwrap();
    let bundle = MentalModelBundle {
        domain: vec![MentalModelDomainRow {
            key: "k".to_owned(),
            description: "d".to_owned(),
            learned_at_unix: 0,
            confidence: 0.5,
        }],
        ..Default::default()
    };
    store.mental_model_save("f1", &bundle).await.unwrap();
    store.mental_model_clear("f1").await.unwrap();
    let loaded = store.mental_model_load("f1").await.unwrap();
    assert!(loaded.domain.is_empty());
}
