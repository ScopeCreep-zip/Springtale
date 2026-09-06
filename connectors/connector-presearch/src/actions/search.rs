use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::cache::ResultCache;
use crate::client::PresearchApi;
use crate::error::PresearchError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        destructive: None,
        poll_interval_secs: None,
        name: "search".to_owned(),
        description: "Search the web using Presearch's privacy-first decentralized search engine."
            .to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query."
                }
            },
            "required": ["query"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "results": {
                    "type": "object",
                    "description": "Raw search results from the Presearch API."
                },
                "cached": {
                    "type": "boolean",
                    "description": "Whether the result was served from cache."
                }
            },
            "required": ["results", "cached"]
        })),
    }
}

pub async fn execute(
    client: &dyn PresearchApi,
    cache: &ResultCache,
    input: &serde_json::Value,
) -> Result<ActionResult, PresearchError> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PresearchError::InvalidInput("missing 'query' parameter".to_owned()))?;

    // Check cache first
    let cache_key = format!("search:{query}");
    if let Some(cached) = cache.get(&cache_key).await {
        tracing::debug!(query = query, "serving search results from cache");
        return Ok(ActionResult {
            success: true,
            output: serde_json::json!({
                "results": cached,
                "cached": true,
            }),
            message: format!("search for '{query}' (cached)"),
        });
    }

    let results = client.search(query).await?;

    // Store in cache
    cache.insert(cache_key, results.clone()).await;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "results": results,
            "cached": false,
        }),
        message: format!("search for '{query}'"),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::PresearchClient;
    use crate::client::test_helpers::MockPresearchClient;

    fn real_test_client() -> PresearchClient {
        let config = crate::config::PresearchConfig {
            api_key: secrecy::SecretBox::new(Box::new("fake".to_owned())),
            api_base: "http://localhost:0".to_owned(),
            cache_ttl_secs: 60,
            allowed_scrape_hosts: vec![],
        };
        PresearchClient::new(&config).unwrap()
    }

    fn test_cache() -> ResultCache {
        ResultCache::new(std::time::Duration::from_secs(60))
    }

    #[test]
    fn test_declaration() {
        let decl = declaration();
        assert_eq!(decl.name, "search");
        let schema = decl.input_schema.unwrap();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("query")));
    }

    // --- Input validation tests (use real client, never reaches network) ---

    #[tokio::test]
    async fn test_execute_missing_query_returns_invalid_input() {
        let client = real_test_client();
        let cache = test_cache();
        let input = serde_json::json!({});
        let result = execute(&client, &cache, &input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PresearchError::InvalidInput(_)
        ));
    }

    #[tokio::test]
    async fn test_execute_query_not_a_string_returns_invalid_input() {
        let client = real_test_client();
        let cache = test_cache();
        let input = serde_json::json!({ "query": 42 });
        let result = execute(&client, &cache, &input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PresearchError::InvalidInput(_)
        ));
    }

    #[tokio::test]
    async fn test_execute_null_query_returns_invalid_input() {
        let client = real_test_client();
        let cache = test_cache();
        let input = serde_json::json!({ "query": null });
        let result = execute(&client, &cache, &input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PresearchError::InvalidInput(_)
        ));
    }

    // --- Mock client tests: verify response extraction and cache interaction ---

    #[tokio::test]
    async fn test_execute_returns_search_results_uncached() {
        let mock = MockPresearchClient::for_search(serde_json::json!({
            "results": [
                { "title": "Result 1", "url": "https://example.com/1" },
                { "title": "Result 2", "url": "https://example.com/2" }
            ]
        }));

        let cache = test_cache();
        let input = serde_json::json!({ "query": "rust programming" });

        let result = execute(&mock, &cache, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["cached"], false);
        assert_eq!(result.output["results"]["results"][0]["title"], "Result 1");
        assert!(result.message.contains("rust programming"));
    }

    #[tokio::test]
    async fn test_execute_returns_cached_result_on_second_call() {
        let mock = MockPresearchClient::for_search(serde_json::json!({ "items": ["a", "b"] }));

        let cache = test_cache();
        let input = serde_json::json!({ "query": "cached query" });

        // First call — should not be cached
        let result1 = execute(&mock, &cache, &input).await.unwrap();
        assert_eq!(result1.output["cached"], false);

        // Second call — should be served from cache
        let result2 = execute(&mock, &cache, &input).await.unwrap();
        assert_eq!(result2.output["cached"], true);
        assert_eq!(result2.output["results"], result1.output["results"]);
    }

    #[tokio::test]
    async fn test_execute_stores_result_in_cache() {
        let mock = MockPresearchClient::for_search(serde_json::json!({ "count": 5 }));

        let cache = test_cache();
        let input = serde_json::json!({ "query": "test" });

        execute(&mock, &cache, &input).await.unwrap();

        // Verify the cache has an entry
        let cached = cache.get("search:test").await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap()["count"], 5);
    }
}
