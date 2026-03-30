use std::collections::HashMap;

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::HttpApi;
use crate::error::HttpError;

/// Action declaration for `post`.
pub fn declaration() -> ActionDecl {
    ActionDecl {
        name: "post".to_owned(),
        description: "Send an HTTP POST request to an allow-listed host.".to_owned(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to request."
                },
                "headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Additional headers to include in the request.",
                    "default": {}
                },
                "body": {
                    "type": "string",
                    "description": "Request body content.",
                    "default": ""
                }
            },
            "required": ["url"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "status": { "type": "integer", "description": "HTTP status code." },
                "headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Response headers."
                },
                "body": { "type": "string", "description": "Response body as text." }
            },
            "required": ["status", "headers", "body"]
        })),
    }
}

/// Execute the `post` action.
pub async fn execute(
    client: &dyn HttpApi,
    input: &serde_json::Value,
) -> Result<ActionResult, HttpError> {
    let url = input
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| HttpError::InvalidInput("missing 'url' parameter".to_owned()))?;

    let headers = parse_headers(input);
    let body = input.get("body").and_then(|v| v.as_str()).unwrap_or("");

    let response = client.post(url, &headers, body).await?;

    Ok(ActionResult {
        success: response.status < 400,
        output: serde_json::json!({
            "status": response.status,
            "headers": response.headers,
            "body": response.body,
        }),
        message: format!("POST {url} => {}", response.status),
    })
}

/// Parse headers from input JSON.
fn parse_headers(input: &serde_json::Value) -> HashMap<String, String> {
    input
        .get("headers")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::client::test_helpers::MockHttpClient;
    use crate::client::{HttpClient, HttpResponse};

    fn real_test_client() -> HttpClient {
        let config = crate::config::HttpConfig {
            allowed_hosts: vec!["example.com".to_owned()],
            default_headers: std::collections::HashMap::new(),
            timeout_secs: 5,
        };
        HttpClient::new(config).unwrap()
    }

    #[test]
    fn test_declaration_has_required_url() {
        let decl = declaration();
        assert_eq!(decl.name, "post");
        let schema = decl.input_schema.unwrap();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("url")));
    }

    // --- Input validation tests (use real client, never reaches network) ---

    #[tokio::test]
    async fn test_execute_missing_url_returns_invalid_input() {
        let client = real_test_client();
        let input = serde_json::json!({ "body": "hello" });
        let result = execute(&client, &input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HttpError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_url_not_a_string_returns_invalid_input() {
        let client = real_test_client();
        let input = serde_json::json!({ "url": 123 });
        let result = execute(&client, &input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HttpError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_null_url_returns_invalid_input() {
        let client = real_test_client();
        let input = serde_json::json!({ "url": null });
        let result = execute(&client, &input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HttpError::InvalidInput(_)));
    }

    // --- Mock client tests: verify response extraction logic ---

    #[tokio::test]
    async fn test_execute_success_extracts_status_headers_body() {
        let mut response_headers = HashMap::new();
        response_headers.insert("content-type".to_owned(), "application/json".to_owned());

        let mock = MockHttpClient {
            response: HttpResponse {
                status: 201,
                headers: response_headers,
                body: r#"{"id": 1}"#.to_owned(),
            },
        };

        let input = serde_json::json!({
            "url": "https://example.com/api/items",
            "headers": { "Content-Type": "application/json" },
            "body": r#"{"name": "test"}"#
        });

        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["status"], 201);
        assert_eq!(result.output["body"], r#"{"id": 1}"#);
        assert_eq!(result.output["headers"]["content-type"], "application/json");
        assert!(result.message.contains("POST"));
        assert!(result.message.contains("201"));
    }

    #[tokio::test]
    async fn test_execute_client_error_status_not_success() {
        let mock = MockHttpClient {
            response: HttpResponse {
                status: 400,
                headers: HashMap::new(),
                body: "Bad Request".to_owned(),
            },
        };

        let input = serde_json::json!({ "url": "https://example.com/api" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.output["status"], 400);
        assert_eq!(result.output["body"], "Bad Request");
    }

    #[tokio::test]
    async fn test_execute_default_empty_body() {
        let mock = MockHttpClient {
            response: HttpResponse {
                status: 200,
                headers: HashMap::new(),
                body: "OK".to_owned(),
            },
        };

        // No "body" field in input — should default to ""
        let input = serde_json::json!({ "url": "https://example.com/api" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["status"], 200);
    }
}
