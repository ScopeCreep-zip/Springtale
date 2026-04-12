#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::backend::trait_::StorageBackend;

#[tokio::test]
async fn test_insert_and_list_rules() {
    let backend = InMemoryBackend::new();
    let rules = backend.list_rules().await.unwrap();
    assert!(rules.is_empty());
}

#[tokio::test]
async fn test_session_upsert_and_get() {
    let backend = InMemoryBackend::new();
    let session = SessionRow {
        user_id: "U123".to_owned(),
        channel_id: "C456".to_owned(),
        last_bot_message: None,
        pending_command: None,
        state_data: "{}".to_owned(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    backend.upsert_session(&session).await.unwrap();
    let retrieved = backend.get_session("U123", "C456").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().user_id, "U123");
}

#[tokio::test]
async fn test_session_delete() {
    let backend = InMemoryBackend::new();
    let session = SessionRow {
        user_id: "U123".to_owned(),
        channel_id: "C456".to_owned(),
        last_bot_message: None,
        pending_command: None,
        state_data: "{}".to_owned(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    backend.upsert_session(&session).await.unwrap();
    backend.delete_session("U123", "C456").await.unwrap();
    let retrieved = backend.get_session("U123", "C456").await.unwrap();
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_alias_crud() {
    let backend = InMemoryBackend::new();
    backend.upsert_alias("hi", "help", "user1").await.unwrap();
    let aliases = backend.list_aliases().await.unwrap();
    assert_eq!(aliases.len(), 1);
    assert_eq!(aliases[0].0, "hi");
    assert_eq!(aliases[0].1, "help");

    backend.delete_alias("hi").await.unwrap();
    let aliases = backend.list_aliases().await.unwrap();
    assert!(aliases.is_empty());
}

#[tokio::test]
async fn test_connector_register_and_list() {
    let backend = InMemoryBackend::new();
    let row = ConnectorRow {
        name: "test-connector".to_owned(),
        version: "0.1.0".to_owned(),
        author: "test".to_owned(),
        description: "test connector".to_owned(),
        manifest_json: "{}".to_owned(),
        enabled: true,
        installed_at: Utc::now(),
    };
    backend.register_connector(&row).await.unwrap();
    let connectors = backend.list_connectors().await.unwrap();
    assert_eq!(connectors.len(), 1);
    assert_eq!(connectors[0].name, "test-connector");
}
