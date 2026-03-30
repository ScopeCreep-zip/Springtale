use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

/// A simple TTL-based in-memory cache for search results.
///
/// Entries expire after the configured TTL. This prevents redundant API
/// calls for repeated queries within a short window.
pub struct ResultCache {
    ttl: Duration,
    entries: Mutex<HashMap<String, CacheEntry>>,
}

struct CacheEntry {
    value: serde_json::Value,
    inserted_at: Instant,
}

impl ResultCache {
    /// Create a new cache with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Get a cached value if it exists and hasn't expired.
    pub async fn get(&self, key: &str) -> Option<serde_json::Value> {
        let entries = self.entries.lock().await;
        if let Some(entry) = entries.get(key)
            && entry.inserted_at.elapsed() < self.ttl
        {
            return Some(entry.value.clone());
        }
        None
    }

    /// Insert a value into the cache.
    pub async fn insert(&self, key: String, value: serde_json::Value) {
        let mut entries = self.entries.lock().await;
        entries.insert(
            key,
            CacheEntry {
                value,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Remove expired entries from the cache.
    pub async fn evict_expired(&self) {
        let mut entries = self.entries.lock().await;
        entries.retain(|_, entry| entry.inserted_at.elapsed() < self.ttl);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_hit() {
        let cache = ResultCache::new(Duration::from_secs(60));
        cache
            .insert("query".to_owned(), serde_json::json!({"results": []}))
            .await;

        let result = cache.get("query").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), serde_json::json!({"results": []}));
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = ResultCache::new(Duration::from_secs(60));
        let result = cache.get("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_expiry() {
        let cache = ResultCache::new(Duration::from_millis(50));
        cache
            .insert("query".to_owned(), serde_json::json!({"results": []}))
            .await;

        // Wait for expiry
        tokio::time::sleep(Duration::from_millis(100)).await;

        let result = cache.get("query").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_evict_expired() {
        let cache = ResultCache::new(Duration::from_millis(50));
        cache
            .insert("old".to_owned(), serde_json::json!("old"))
            .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        cache
            .insert("new".to_owned(), serde_json::json!("new"))
            .await;
        cache.evict_expired().await;

        let entries = cache.entries.lock().await;
        assert_eq!(entries.len(), 1);
        assert!(entries.contains_key("new"));
    }
}
