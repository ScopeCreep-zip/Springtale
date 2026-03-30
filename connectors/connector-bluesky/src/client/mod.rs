use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretBox};
use tokio::sync::RwLock;

use crate::config::BlueskyConfig;
use crate::error::BlueskyError;

/// Trait defining the Bluesky ATProto API surface.
///
/// Actions depend on this trait, not the concrete client. This enables
/// mock implementations in tests (per testing.md: "mock at the client
/// layer, not at reqwest level").
#[async_trait]
pub trait BlueskyApi: Send + Sync {
    /// Create a post (app.bsky.feed.post).
    async fn create_post(&self, text: &str) -> Result<serde_json::Value, BlueskyError>;

    /// Reply to a post.
    async fn reply(
        &self,
        text: &str,
        parent_uri: &str,
        parent_cid: &str,
        root_uri: &str,
        root_cid: &str,
    ) -> Result<serde_json::Value, BlueskyError>;

    /// Like a post.
    async fn like(
        &self,
        subject_uri: &str,
        subject_cid: &str,
    ) -> Result<serde_json::Value, BlueskyError>;

    /// Repost a post.
    async fn repost(
        &self,
        subject_uri: &str,
        subject_cid: &str,
    ) -> Result<serde_json::Value, BlueskyError>;
}

/// ATProto session tokens.
///
/// JWTs are wrapped in `SecretBox` — never logged or exposed in Debug.
pub struct Session {
    access_jwt: SecretBox<String>,
    refresh_jwt: SecretBox<String>,
    pub did: String,
    pub handle: String,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("access_jwt", &"[REDACTED]")
            .field("refresh_jwt", &"[REDACTED]")
            .field("did", &self.did)
            .field("handle", &self.handle)
            .finish()
    }
}

/// ATProto client with session management.
///
/// Handles authentication via `createSession`, session refresh via
/// `refreshSession`, and provides methods for creating records.
pub struct AtProtoClient {
    inner: reqwest::Client,
    pds_base: String,
    session: RwLock<Option<Session>>,
}

impl AtProtoClient {
    /// Create a new ATProto client and authenticate.
    pub async fn new(config: &BlueskyConfig) -> Result<Self, BlueskyError> {
        let inner = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| BlueskyError::AtProtoError(format!("failed to build client: {e}")))?;

        let client = Self {
            inner,
            pds_base: config.pds_base.clone(),
            session: RwLock::new(None),
        };

        client.create_session(config).await?;
        Ok(client)
    }

    /// Authenticate and create a session.
    async fn create_session(&self, config: &BlueskyConfig) -> Result<(), BlueskyError> {
        let url = format!("{}/xrpc/com.atproto.server.createSession", self.pds_base);

        // SECURITY: expose needed for ATProto authentication
        let response = self
            .inner
            .post(&url)
            .json(&serde_json::json!({
                "identifier": config.identifier,
                "password": config.password.expose_secret(),
            }))
            .send()
            .await
            .map_err(|e| BlueskyError::AtProtoError(format!("session creation failed: {e}")))?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| BlueskyError::AtProtoError(format!("failed to read response: {e}")))?;

        if status >= 400 {
            return Err(BlueskyError::AtProtoError(format!(
                "createSession failed ({status}): {body}"
            )));
        }

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| BlueskyError::AtProtoError(format!("failed to parse session: {e}")))?;

        let session = Session {
            access_jwt: SecretBox::new(Box::new(
                json["accessJwt"].as_str().unwrap_or_default().to_owned(),
            )),
            refresh_jwt: SecretBox::new(Box::new(
                json["refreshJwt"].as_str().unwrap_or_default().to_owned(),
            )),
            did: json["did"].as_str().unwrap_or_default().to_owned(),
            handle: json["handle"].as_str().unwrap_or_default().to_owned(),
        };

        *self.session.write().await = Some(session);
        tracing::info!("ATProto session created");
        Ok(())
    }

    /// Refresh the current session using the refresh JWT.
    pub async fn refresh_session(&self) -> Result<(), BlueskyError> {
        // SECURITY: expose needed for Bearer auth on session refresh
        let refresh_jwt = {
            let session = self.session.read().await;
            session
                .as_ref()
                .map(|s| s.refresh_jwt.expose_secret().clone())
                .ok_or_else(|| BlueskyError::AtProtoError("no active session".to_owned()))?
        };

        let url = format!("{}/xrpc/com.atproto.server.refreshSession", self.pds_base);

        let response = self
            .inner
            .post(&url)
            .bearer_auth(&refresh_jwt)
            .send()
            .await
            .map_err(|e| BlueskyError::AtProtoError(format!("session refresh failed: {e}")))?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| BlueskyError::AtProtoError(format!("failed to read response: {e}")))?;

        if status >= 400 {
            return Err(BlueskyError::AtProtoError(format!(
                "refreshSession failed ({status}): {body}"
            )));
        }

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| BlueskyError::AtProtoError(format!("failed to parse session: {e}")))?;

        let session = Session {
            access_jwt: SecretBox::new(Box::new(
                json["accessJwt"].as_str().unwrap_or_default().to_owned(),
            )),
            refresh_jwt: SecretBox::new(Box::new(
                json["refreshJwt"].as_str().unwrap_or_default().to_owned(),
            )),
            did: json["did"].as_str().unwrap_or_default().to_owned(),
            handle: json["handle"].as_str().unwrap_or_default().to_owned(),
        };

        *self.session.write().await = Some(session);
        tracing::info!("ATProto session refreshed");
        Ok(())
    }

    /// Get the current access JWT for API calls.
    /// Returns the exposed token string — caller uses it immediately for bearer auth.
    async fn access_jwt(&self) -> Result<String, BlueskyError> {
        let session = self.session.read().await;
        // SECURITY: expose needed for Bearer auth on API calls
        session
            .as_ref()
            .map(|s| s.access_jwt.expose_secret().clone())
            .ok_or_else(|| BlueskyError::AtProtoError("no active session".to_owned()))
    }

    /// Get the DID of the authenticated user.
    pub async fn did(&self) -> Result<String, BlueskyError> {
        let session = self.session.read().await;
        session
            .as_ref()
            .map(|s| s.did.clone())
            .ok_or_else(|| BlueskyError::AtProtoError("no active session".to_owned()))
    }

    /// Create a record via com.atproto.repo.createRecord.
    async fn create_record(
        &self,
        jwt: &str,
        did: &str,
        collection: &str,
        record: serde_json::Value,
    ) -> Result<serde_json::Value, BlueskyError> {
        let url = format!("{}/xrpc/com.atproto.repo.createRecord", self.pds_base);

        let response = self
            .inner
            .post(&url)
            .bearer_auth(jwt)
            .json(&serde_json::json!({
                "repo": did,
                "collection": collection,
                "record": record,
            }))
            .send()
            .await?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| BlueskyError::AtProtoError(format!("failed to read response: {e}")))?;

        if status >= 400 {
            return Err(BlueskyError::AtProtoError(format!(
                "createRecord failed ({status}): {body}"
            )));
        }

        serde_json::from_str(&body)
            .map_err(|e| BlueskyError::AtProtoError(format!("failed to parse response: {e}")))
    }
}

#[async_trait]
impl BlueskyApi for AtProtoClient {
    async fn create_post(&self, text: &str) -> Result<serde_json::Value, BlueskyError> {
        let did = self.did().await?;
        let jwt = self.access_jwt().await?;

        let now = chrono_now();

        let record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": text,
            "createdAt": now,
        });

        self.create_record(&jwt, &did, "app.bsky.feed.post", record)
            .await
    }

    async fn reply(
        &self,
        text: &str,
        parent_uri: &str,
        parent_cid: &str,
        root_uri: &str,
        root_cid: &str,
    ) -> Result<serde_json::Value, BlueskyError> {
        let did = self.did().await?;
        let jwt = self.access_jwt().await?;

        let now = chrono_now();

        let record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": text,
            "createdAt": now,
            "reply": {
                "parent": { "uri": parent_uri, "cid": parent_cid },
                "root": { "uri": root_uri, "cid": root_cid },
            }
        });

        self.create_record(&jwt, &did, "app.bsky.feed.post", record)
            .await
    }

    async fn like(
        &self,
        subject_uri: &str,
        subject_cid: &str,
    ) -> Result<serde_json::Value, BlueskyError> {
        let did = self.did().await?;
        let jwt = self.access_jwt().await?;

        let now = chrono_now();

        let record = serde_json::json!({
            "$type": "app.bsky.feed.like",
            "subject": { "uri": subject_uri, "cid": subject_cid },
            "createdAt": now,
        });

        self.create_record(&jwt, &did, "app.bsky.feed.like", record)
            .await
    }

    async fn repost(
        &self,
        subject_uri: &str,
        subject_cid: &str,
    ) -> Result<serde_json::Value, BlueskyError> {
        let did = self.did().await?;
        let jwt = self.access_jwt().await?;

        let now = chrono_now();

        let record = serde_json::json!({
            "$type": "app.bsky.feed.repost",
            "subject": { "uri": subject_uri, "cid": subject_cid },
            "createdAt": now,
        });

        self.create_record(&jwt, &did, "app.bsky.feed.repost", record)
            .await
    }
}

/// Generate an ISO 8601 timestamp for ATProto records.
fn chrono_now() -> String {
    // Manual ISO 8601 using std — avoids chrono dependency
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Simple UTC timestamp: we compute year/month/day/hour/min/sec from epoch
    // For production, a proper datetime library is better, but this avoids
    // adding a dependency for Phase 1a.
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Compute date from days since epoch (1970-01-01)
    let (year, month, day) = days_to_date(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.000Z")
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Based on Howard Hinnant's algorithm
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    /// Configurable mock for `BlueskyApi`.
    ///
    /// Set the `response` field to the JSON value the mock should return.
    /// All trait methods return `self.response.clone()`.
    pub struct MockBlueskyClient {
        pub response: serde_json::Value,
    }

    #[async_trait]
    impl BlueskyApi for MockBlueskyClient {
        async fn create_post(&self, _text: &str) -> Result<serde_json::Value, BlueskyError> {
            Ok(self.response.clone())
        }

        async fn reply(
            &self,
            _text: &str,
            _parent_uri: &str,
            _parent_cid: &str,
            _root_uri: &str,
            _root_cid: &str,
        ) -> Result<serde_json::Value, BlueskyError> {
            Ok(self.response.clone())
        }

        async fn like(
            &self,
            _subject_uri: &str,
            _subject_cid: &str,
        ) -> Result<serde_json::Value, BlueskyError> {
            Ok(self.response.clone())
        }

        async fn repost(
            &self,
            _subject_uri: &str,
            _subject_cid: &str,
        ) -> Result<serde_json::Value, BlueskyError> {
            Ok(self.response.clone())
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_chrono_now_format() {
        let now = chrono_now();
        // Should be ISO 8601 format
        assert!(now.ends_with('Z'));
        assert!(now.contains('T'));
        assert_eq!(now.len(), 24); // YYYY-MM-DDTHH:MM:SS.000Z
    }

    #[test]
    fn test_days_to_date_epoch() {
        let (y, m, d) = days_to_date(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_date_known() {
        // 2024-01-01 is day 19723
        let (y, m, d) = days_to_date(19723);
        assert_eq!(y, 2024);
        assert_eq!(m, 1);
        assert_eq!(d, 1);
    }
}
