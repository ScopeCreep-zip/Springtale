use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::cache::ResultCache;
use crate::client::PresearchApi;
use crate::error::PresearchError;

pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "scrape".to_owned(),
        description: "Fetch the text content of a URL.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch."
                }
            },
            "required": ["url"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The page content as text."
                },
                "cached": {
                    "type": "boolean",
                    "description": "Whether the result was served from cache."
                }
            },
            "required": ["content", "cached"]
        })),
    }
}

pub async fn execute(
    client: &dyn PresearchApi,
    cache: &ResultCache,
    input: &serde_json::Value,
) -> Result<ActionResult, PresearchError> {
    let url = input
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PresearchError::InvalidInput("missing 'url' parameter".to_owned()))?;

    // Check cache first
    let cache_key = format!("scrape:{url}");
    if let Some(cached) = cache.get(&cache_key).await {
        tracing::debug!(url = url, "serving scraped content from cache");
        let content = cached.as_str().unwrap_or_default();
        return Ok(ActionResult {
            success: true,
            output: serde_json::json!({
                "content": content,
                "cached": true,
            }),
            message: format!("scraped {url} (cached)"),
        });
    }

    let content = client.fetch_url(url).await?;

    // Store in cache
    cache
        .insert(cache_key, serde_json::Value::String(content.clone()))
        .await;

    Ok(ActionResult {
        success: true,
        output: serde_json::json!({
            "content": content,
            "cached": false,
        }),
        message: format!("scraped {url}"),
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
        assert_eq!(decl.name, "scrape");
        let schema = decl.input_schema.unwrap();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("url")));
    }

    // --- Input validation tests (use real client, never reaches network) ---

    #[tokio::test]
    async fn test_execute_missing_url_returns_invalid_input() {
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
    async fn test_execute_url_not_a_string_returns_invalid_input() {
        let client = real_test_client();
        let cache = test_cache();
        let input = serde_json::json!({ "url": 123 });
        let result = execute(&client, &cache, &input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PresearchError::InvalidInput(_)
        ));
    }

    #[tokio::test]
    async fn test_execute_null_url_returns_invalid_input() {
        let client = real_test_client();
        let cache = test_cache();
        let input = serde_json::json!({ "url": null });
        let result = execute(&client, &cache, &input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PresearchError::InvalidInput(_)
        ));
    }

    // --- Mock client tests: verify response extraction and cache interaction ---

    #[tokio::test]
    async fn test_execute_returns_scraped_content_uncached() {
        let mock =
            MockPresearchClient::for_fetch("<html><body>Hello World</body></html>".to_owned());

        let cache = test_cache();
        let input = serde_json::json!({ "url": "https://example.com/page" });

        let result = execute(&mock, &cache, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["cached"], false);
        assert_eq!(
            result.output["content"],
            "<html><body>Hello World</body></html>"
        );
        assert!(result.message.contains("example.com"));
    }

    #[tokio::test]
    async fn test_execute_returns_cached_result_on_second_call() {
        let mock = MockPresearchClient::for_fetch("page content".to_owned());

        let cache = test_cache();
        let input = serde_json::json!({ "url": "https://example.com/cached" });

        // First call — should not be cached
        let result1 = execute(&mock, &cache, &input).await.unwrap();
        assert_eq!(result1.output["cached"], false);
        assert_eq!(result1.output["content"], "page content");

        // Second call — should be served from cache
        let result2 = execute(&mock, &cache, &input).await.unwrap();
        assert_eq!(result2.output["cached"], true);
        assert_eq!(result2.output["content"], "page content");
    }

    #[tokio::test]
    async fn test_execute_stores_result_in_cache() {
        let mock = MockPresearchClient::for_fetch("cached body".to_owned());

        let cache = test_cache();
        let input = serde_json::json!({ "url": "https://example.com/store" });

        execute(&mock, &cache, &input).await.unwrap();

        // Verify the cache has an entry
        let cached = cache.get("scrape:https://example.com/store").await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), "cached body");
    }
}
