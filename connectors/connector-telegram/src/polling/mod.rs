use std::sync::Arc;

use crate::client::TelegramApi;
use crate::error::TelegramError;

/// Run the long-polling loop for Telegram updates.
///
/// How `getUpdates` works:
/// 1. Call `getUpdates` with current `offset` (starts at None)
/// 2. Telegram holds the connection for `timeout` seconds (long-poll)
/// 3. When updates arrive, Telegram returns them as a JSON array
/// 4. For each update, extract `update_id`
/// 5. Set `offset = highest_update_id + 1` (acknowledges processed updates)
/// 6. Repeat
///
/// The offset acts as an acknowledgment: Telegram will not re-send updates
/// with `update_id < offset`. This ensures at-most-once delivery.
pub async fn polling_loop<C: TelegramApi>(
    client: Arc<C>,
    timeout: u64,
    allowed_updates: Vec<String>,
    dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut offset: Option<i64> = None;

    loop {
        // Check shutdown signal
        if *shutdown.borrow() {
            tracing::info!("polling loop shutting down");
            break;
        }

        match client.get_updates(offset, timeout, &allowed_updates).await {
            Ok(updates) => {
                if let Some(arr) = updates.as_array() {
                    for update in arr {
                        if let Some(id) = update.get("update_id").and_then(|v| v.as_i64()) {
                            offset = Some(id + 1);
                        }
                        dispatcher(update.clone());
                    }
                }
            }
            Err(TelegramError::RateLimited { retry_after }) => {
                tracing::warn!(retry_after, "rate limited by Telegram, backing off");
                tokio::time::sleep(std::time::Duration::from_secs(retry_after)).await;
            }
            Err(e) => {
                tracing::error!(error = %e, "polling error, retrying in 5s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }

        // Check shutdown after each iteration
        if shutdown.has_changed().unwrap_or(false) && *shutdown.borrow_and_update() {
            tracing::info!("polling loop received shutdown signal");
            break;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_polling_processes_updates() {
        use std::sync::atomic::{AtomicBool, Ordering};

        // Mock that returns updates on first call, then empty on subsequent calls
        struct OneShotApi {
            returned: AtomicBool,
        }

        #[async_trait::async_trait]
        impl TelegramApi for OneShotApi {
            async fn send_message(
                &self,
                _: &str,
                _: &str,
                _: Option<&str>,
                _: Option<i64>,
            ) -> Result<serde_json::Value, TelegramError> {
                Ok(serde_json::json!({}))
            }
            async fn send_photo(
                &self,
                _: &str,
                _: &str,
                _: Option<&str>,
            ) -> Result<serde_json::Value, TelegramError> {
                Ok(serde_json::json!({}))
            }
            async fn edit_message_text(
                &self,
                _: &str,
                _: i64,
                _: &str,
                _: Option<&str>,
            ) -> Result<serde_json::Value, TelegramError> {
                Ok(serde_json::json!({}))
            }
            async fn delete_message(
                &self,
                _: &str,
                _: i64,
            ) -> Result<serde_json::Value, TelegramError> {
                Ok(serde_json::json!({}))
            }
            async fn send_inline_keyboard(
                &self,
                _: &str,
                _: &str,
                _: serde_json::Value,
            ) -> Result<serde_json::Value, TelegramError> {
                Ok(serde_json::json!({}))
            }
            async fn set_webhook(
                &self,
                _: &str,
                _: Option<&str>,
                _: &[String],
            ) -> Result<serde_json::Value, TelegramError> {
                Ok(serde_json::json!({}))
            }
            async fn delete_webhook(&self) -> Result<serde_json::Value, TelegramError> {
                Ok(serde_json::json!({}))
            }
            async fn get_me(&self) -> Result<serde_json::Value, TelegramError> {
                Ok(serde_json::json!({}))
            }

            async fn get_updates(
                &self,
                _: Option<i64>,
                _: u64,
                _: &[String],
            ) -> Result<serde_json::Value, TelegramError> {
                if !self.returned.swap(true, Ordering::SeqCst) {
                    Ok(serde_json::json!([
                        { "update_id": 100, "message": { "text": "hello" } },
                        { "update_id": 101, "message": { "text": "world" } }
                    ]))
                } else {
                    // Return empty on subsequent calls; sleep to yield to shutdown
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    Ok(serde_json::json!([]))
                }
            }
        }

        let mock = Arc::new(OneShotApi {
            returned: AtomicBool::new(false),
        });

        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let received_clone = received.clone();
        let dispatcher: Arc<dyn Fn(serde_json::Value) + Send + Sync> = Arc::new(move |update| {
            received_clone.lock().unwrap().push(update);
        });

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let _ = shutdown_tx.send(true);
        });

        polling_loop(mock, 1, vec![], dispatcher, shutdown_rx).await;

        let updates = received.lock().unwrap();
        assert_eq!(updates.len(), 2);
    }
}
