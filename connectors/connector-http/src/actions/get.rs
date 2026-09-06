use std::collections::HashMap;

use springtale_connector::connector::trait_::ActionResult;
use springtale_connector::manifest::types::ActionDecl;

use crate::client::HttpApi;
use crate::error::HttpError;

/// Action declaration for `get`.
pub fn declaration() -> ActionDecl {
    ActionDecl {
        read_only: true,
        destructive: None,
        poll_interval_secs: None,
        name: "get".to_owned(),
        description: "Send an HTTP GET request to an allow-listed host.".to_owned(),
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

/// Execute the `get` action.
pub async fn execute(
    client: &dyn HttpApi,
    input: &serde_json::Value,
) -> Result<ActionResult, HttpError> {
    let url = input
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| HttpError::InvalidInput("missing 'url' parameter".to_owned()))?;

    let headers = parse_headers(input);

    let response = client.get(url, &headers).await?;

    Ok(ActionResult {
        success: response.status < 400,
        output: serde_json::json!({
            "status": response.status,
            "headers": response.headers,
            "body": response.body,
        }),
        message: format!("GET {url} => {}", response.status),
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
        assert_eq!(decl.name, "get");
        let schema = decl.input_schema.unwrap();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("url")));
    }

    #[test]
    fn test_parse_headers_empty() {
        let input = serde_json::json!({ "url": "https://example.com" });
        let headers = parse_headers(&input);
        assert!(headers.is_empty());
    }

    #[test]
    fn test_parse_headers_present() {
        let input = serde_json::json!({
            "url": "https://example.com",
            "headers": {
                "Authorization": "Bearer token123",
                "Accept": "application/json"
            }
        });
        let headers = parse_headers(&input);
        assert_eq!(headers.len(), 2);
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer token123");
    }

    // --- Input validation tests (use real client, never reaches network) ---

    #[tokio::test]
    async fn test_execute_missing_url_returns_invalid_input() {
        let client = real_test_client();
        let input = serde_json::json!({});
        let result = execute(&client, &input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HttpError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_execute_url_not_a_string_returns_invalid_input() {
        let client = real_test_client();
        let input = serde_json::json!({ "url": 42 });
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
        response_headers.insert("content-type".to_owned(), "text/html".to_owned());

        let mock = MockHttpClient {
            response: HttpResponse {
                status: 200,
                headers: response_headers,
                body: "<html>OK</html>".to_owned(),
            },
        };

        let input = serde_json::json!({
            "url": "https://example.com/page",
            "headers": { "Accept": "text/html" }
        });

        let result = execute(&mock, &input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output["status"], 200);
        assert_eq!(result.output["body"], "<html>OK</html>");
        assert_eq!(result.output["headers"]["content-type"], "text/html");
        assert!(result.message.contains("GET"));
        assert!(result.message.contains("200"));
    }

    #[tokio::test]
    async fn test_execute_client_error_status_not_success() {
        let mock = MockHttpClient {
            response: HttpResponse {
                status: 404,
                headers: HashMap::new(),
                body: "Not Found".to_owned(),
            },
        };

        let input = serde_json::json!({ "url": "https://example.com/missing" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.output["status"], 404);
        assert_eq!(result.output["body"], "Not Found");
    }

    #[tokio::test]
    async fn test_execute_server_error_status_not_success() {
        let mock = MockHttpClient {
            response: HttpResponse {
                status: 500,
                headers: HashMap::new(),
                body: "Internal Server Error".to_owned(),
            },
        };

        let input = serde_json::json!({ "url": "https://example.com/error" });
        let result = execute(&mock, &input).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.output["status"], 500);
    }
}
