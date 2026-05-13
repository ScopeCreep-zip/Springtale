use async_trait::async_trait;

use crate::error::SignalError;

/// A Signal addressable target discovered via signal-cli enumeration.
///
/// `kind` is either `"group"` (then `id` is the base64 group id) or
/// `"contact"` (then `id` is the E.164 phone number).
#[derive(Debug, Clone)]
pub struct DiscoveredSignalRecipient {
    pub id: String,
    pub display_name: String,
    pub kind: String,
    pub member_count: Option<u64>,
}

/// Trait defining the Signal API surface (via signal-cli daemon).
/// Actions depend on this trait — enables mock testing.
#[async_trait]
pub trait SignalApi: Send + Sync {
    /// Send a message to one or more recipients.
    async fn send_message(
        &self,
        recipients: &[String],
        message: &str,
    ) -> Result<serde_json::Value, SignalError>;

    /// Send a message to a group.
    async fn send_group_message(
        &self,
        group_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, SignalError>;

    /// Set the disappearing message timer for a 1:1 conversation.
    async fn set_disappearing_timer(
        &self,
        recipient: &str,
        expires_in_seconds: u64,
    ) -> Result<(), SignalError>;

    /// Enumerate every addressable Signal target — groups via
    /// `listGroups` and 1:1 contacts via `listContacts` (both JSON-RPC
    /// methods on signal-cli).
    async fn list_destinations(
        &self,
    ) -> Result<Vec<DiscoveredSignalRecipient>, SignalError>;
}

/// Concrete Signal client bridging to signal-cli daemon via HTTP JSON-RPC.
///
/// signal-cli runs as a separate process. This client communicates via
/// `POST /api/v1/rpc` with JSON-RPC 2.0 format.
///
/// No credentials stored here — signal-cli handles all Signal Protocol
/// authentication. Phone number stays in signal-cli's local data only.
pub struct SignalClient {
    http: reqwest::Client,
    daemon_url: String,
    account_id: String,
    jitter_secs: u64,
    next_id: std::sync::atomic::AtomicU64,
}

impl SignalClient {
    pub fn new(daemon_url: String, account_id: String, jitter_secs: u64) -> Self {
        let http = reqwest::Client::new();
        Self {
            http,
            daemon_url,
            account_id,
            jitter_secs,
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    async fn apply_jitter(&self) {
        if self.jitter_secs > 0 {
            let jitter = rand::random::<u64>() % self.jitter_secs;
            tokio::time::sleep(std::time::Duration::from_secs(jitter)).await;
        }
    }

    fn next_request_id(&self) -> String {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        id.to_string()
    }

    /// Send a JSON-RPC request to the signal-cli daemon.
    async fn rpc_call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, SignalError> {
        let url = format!("{}/api/v1/rpc", self.daemon_url);
        let request_id = self.next_request_id();

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "id": request_id,
            "params": params,
        });

        let response = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| SignalError::DaemonUnreachable(format!("{method}: {e}")))?;

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| SignalError::ApiError(format!("{method} response parse failed: {e}")))?;

        // Check for JSON-RPC error
        if let Some(error) = json.get("error") {
            let msg = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
            return Err(SignalError::ApiError(format!(
                "{method}: error {code}: {msg}"
            )));
        }

        Ok(json
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Access the daemon URL (for gateway SSE endpoint).
    pub fn daemon_url(&self) -> &str {
        &self.daemon_url
    }
}

#[async_trait]
impl SignalApi for SignalClient {
    async fn send_message(
        &self,
        recipients: &[String],
        message: &str,
    ) -> Result<serde_json::Value, SignalError> {
        self.apply_jitter().await;
        let params = serde_json::json!({
            "account": self.account_id,
            "recipients": recipients,
            "message": message,
        });
        self.rpc_call("send", params).await
    }

    async fn send_group_message(
        &self,
        group_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, SignalError> {
        self.apply_jitter().await;
        let params = serde_json::json!({
            "account": self.account_id,
            "groupId": group_id,
            "message": message,
        });
        self.rpc_call("send", params).await
    }

    async fn set_disappearing_timer(
        &self,
        recipient: &str,
        expires_in_seconds: u64,
    ) -> Result<(), SignalError> {
        self.apply_jitter().await;
        let params = serde_json::json!({
            "account": self.account_id,
            "recipient": recipient,
            "expiresInSeconds": expires_in_seconds,
        });
        self.rpc_call("setExpirationTimer", params).await?;
        Ok(())
    }

    async fn list_destinations(
        &self,
    ) -> Result<Vec<DiscoveredSignalRecipient>, SignalError> {
        let mut out = Vec::new();

        // Groups: signal-cli `listGroups` returns
        // [{"id": "<base64>", "name": "...", "members": [...], ...}, ...]
        let params = serde_json::json!({ "account": self.account_id });
        let groups = self.rpc_call("listGroups", params).await?;
        if let Some(arr) = groups.as_array() {
            for g in arr {
                let id = match g.get("id").and_then(|v| v.as_str()) {
                    Some(s) => s.to_owned(),
                    None => continue,
                };
                let name = g
                    .get("name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Signal group")
                    .to_owned();
                let member_count = g
                    .get("members")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len() as u64);
                out.push(DiscoveredSignalRecipient {
                    id,
                    display_name: name,
                    kind: "group".to_owned(),
                    member_count,
                });
            }
        }

        // Contacts: signal-cli `listContacts` returns
        // [{"number": "+1...", "name": "...", "uuid": "...", ...}, ...]
        let params = serde_json::json!({ "account": self.account_id });
        let contacts = self.rpc_call("listContacts", params).await?;
        if let Some(arr) = contacts.as_array() {
            for c in arr {
                let phone = match c.get("number").and_then(|v| v.as_str()) {
                    Some(s) if !s.is_empty() => s.to_owned(),
                    _ => continue,
                };
                let name = c
                    .get("name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&phone)
                    .to_owned();
                out.push(DiscoveredSignalRecipient {
                    id: phone,
                    display_name: name,
                    kind: "contact".to_owned(),
                    member_count: None,
                });
            }
        }

        Ok(out)
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    pub struct MockSignalApi;

    #[async_trait]
    impl SignalApi for MockSignalApi {
        async fn send_message(
            &self,
            _recipients: &[String],
            _message: &str,
        ) -> Result<serde_json::Value, SignalError> {
            Ok(serde_json::json!({ "timestamp": 1234567890 }))
        }

        async fn send_group_message(
            &self,
            _group_id: &str,
            _message: &str,
        ) -> Result<serde_json::Value, SignalError> {
            Ok(serde_json::json!({ "timestamp": 1234567890 }))
        }

        async fn set_disappearing_timer(
            &self,
            _recipient: &str,
            _expires_in_seconds: u64,
        ) -> Result<(), SignalError> {
            Ok(())
        }

        async fn list_destinations(
            &self,
        ) -> Result<Vec<DiscoveredSignalRecipient>, SignalError> {
            Ok(vec![
                DiscoveredSignalRecipient {
                    id: "GROUP_ID_BASE64=".to_owned(),
                    display_name: "Coordinating Cell".to_owned(),
                    kind: "group".to_owned(),
                    member_count: Some(8),
                },
                DiscoveredSignalRecipient {
                    id: "GROUP_ID_BASE64_2=".to_owned(),
                    display_name: "Signal group".to_owned(),
                    kind: "group".to_owned(),
                    member_count: Some(2),
                },
                DiscoveredSignalRecipient {
                    id: "+15551234567".to_owned(),
                    display_name: "Alice".to_owned(),
                    kind: "contact".to_owned(),
                    member_count: None,
                },
                DiscoveredSignalRecipient {
                    id: "+15559876543".to_owned(),
                    display_name: "+15559876543".to_owned(),
                    kind: "contact".to_owned(),
                    member_count: None,
                },
            ])
        }
    }
}
