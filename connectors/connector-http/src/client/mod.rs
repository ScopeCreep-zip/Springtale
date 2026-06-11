use std::collections::HashMap;

use async_trait::async_trait;

use crate::config::HttpConfig;
use crate::error::HttpError;

/// Trait defining the HTTP API surface.
///
/// Actions depend on this trait, not the concrete client. This enables
/// mock implementations in tests (per testing.md: "mock at the client
/// layer, not at reqwest level").
#[async_trait]
pub trait HttpApi: Send + Sync {
    async fn get(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
    ) -> Result<HttpResponse, HttpError>;

    async fn post(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        body: &str,
    ) -> Result<HttpResponse, HttpError>;
}

/// HTTP client that enforces host allow-list.
///
/// All network calls in the HTTP connector go through this client.
/// The client validates the target host against the allow-list before
/// making any request.
pub struct HttpClient {
    inner: reqwest::Client,
    config: HttpConfig,
}

/// Response from an HTTP request.
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl HttpClient {
    /// Create a new HTTP client with the given config.
    pub fn new(config: HttpConfig) -> Result<Self, HttpError> {
        let mut default_headers = reqwest::header::HeaderMap::new();
        for (key, value) in &config.default_headers {
            let header_name =
                reqwest::header::HeaderName::from_bytes(key.as_bytes()).map_err(|e| {
                    HttpError::InvalidConfig(format!("invalid header name '{key}': {e}"))
                })?;
            let header_value = reqwest::header::HeaderValue::from_str(value).map_err(|e| {
                HttpError::InvalidConfig(format!("invalid header value for '{key}': {e}"))
            })?;
            default_headers.insert(header_name, header_value);
        }

        let inner = springtale_transport::safe_http::builder()
            .timeout(config.timeout_duration())
            .default_headers(default_headers)
            .build()
            .map_err(|e| HttpError::RequestFailed(format!("failed to build HTTP client: {e}")))?;

        Ok(Self { inner, config })
    }

    /// Validate that a URL's host is in the allow-list.
    fn validate_host(&self, url: &str) -> Result<(), HttpError> {
        let parsed =
            reqwest::Url::parse(url).map_err(|e| HttpError::InvalidUrl(format!("{url}: {e}")))?;

        let host = parsed
            .host_str()
            .ok_or_else(|| HttpError::InvalidUrl(format!("no host in URL: {url}")))?;

        if !self.config.is_host_allowed(host) {
            return Err(HttpError::HostNotAllowed(host.to_owned()));
        }

        Ok(())
    }

    /// Send a GET request (internal implementation).
    async fn do_get(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
    ) -> Result<HttpResponse, HttpError> {
        self.validate_host(url)?;

        let mut request = self.inner.get(url);
        for (key, value) in headers {
            request = request.header(key.as_str(), value.as_str());
        }

        tracing::info!(url = url, method = "GET", "sending HTTP request");
        let response = request.send().await?;
        to_http_response(response).await
    }

    /// Send a POST request (internal implementation).
    async fn do_post(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        body: &str,
    ) -> Result<HttpResponse, HttpError> {
        self.validate_host(url)?;

        let mut request = self.inner.post(url).body(body.to_owned());
        for (key, value) in headers {
            request = request.header(key.as_str(), value.as_str());
        }

        tracing::info!(url = url, method = "POST", "sending HTTP request");
        let response = request.send().await?;
        to_http_response(response).await
    }
}

#[async_trait]
impl HttpApi for HttpClient {
    async fn get(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
    ) -> Result<HttpResponse, HttpError> {
        self.do_get(url, headers).await
    }

    async fn post(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        body: &str,
    ) -> Result<HttpResponse, HttpError> {
        self.do_post(url, headers, body).await
    }
}

/// Convert a reqwest response into our `HttpResponse`.
async fn to_http_response(response: reqwest::Response) -> Result<HttpResponse, HttpError> {
    let status = response.status().as_u16();

    let mut headers = HashMap::new();
    for (key, value) in response.headers() {
        if let Ok(v) = value.to_str() {
            headers.insert(key.to_string(), v.to_owned());
        }
    }

    let body = response
        .text()
        .await
        .map_err(|e| HttpError::ParseError(format!("failed to read response body: {e}")))?;

    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    /// Configurable mock for `HttpApi`.
    ///
    /// Set the `response` field to the `HttpResponse` the mock should return.
    /// All trait methods return a clone of that response.
    pub struct MockHttpClient {
        pub response: HttpResponse,
    }

    #[async_trait]
    impl HttpApi for MockHttpClient {
        async fn get(
            &self,
            _url: &str,
            _headers: &HashMap<String, String>,
        ) -> Result<HttpResponse, HttpError> {
            Ok(HttpResponse {
                status: self.response.status,
                headers: self.response.headers.clone(),
                body: self.response.body.clone(),
            })
        }

        async fn post(
            &self,
            _url: &str,
            _headers: &HashMap<String, String>,
            _body: &str,
        ) -> Result<HttpResponse, HttpError> {
            Ok(HttpResponse {
                status: self.response.status,
                headers: self.response.headers.clone(),
                body: self.response.body.clone(),
            })
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_host_allowed() {
        let config = HttpConfig {
            allowed_hosts: vec!["api.example.com".to_owned()],
            default_headers: HashMap::new(),
            timeout_secs: 30,
        };
        let client = HttpClient::new(config).unwrap();

        assert!(client.validate_host("https://api.example.com/path").is_ok());
    }

    #[test]
    fn test_validate_host_rejected() {
        let config = HttpConfig {
            allowed_hosts: vec!["api.example.com".to_owned()],
            default_headers: HashMap::new(),
            timeout_secs: 30,
        };
        let client = HttpClient::new(config).unwrap();

        let result = client.validate_host("https://evil.com/path");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HttpError::HostNotAllowed(_)));
    }

    #[test]
    fn test_validate_host_invalid_url() {
        let config = HttpConfig {
            allowed_hosts: vec![],
            default_headers: HashMap::new(),
            timeout_secs: 30,
        };
        let client = HttpClient::new(config).unwrap();

        let result = client.validate_host("not a url");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HttpError::InvalidUrl(_)));
    }

    #[test]
    fn test_client_with_default_headers() {
        let mut headers = HashMap::new();
        headers.insert("User-Agent".to_owned(), "Springtale/1.0".to_owned());

        let config = HttpConfig {
            allowed_hosts: vec![],
            default_headers: headers,
            timeout_secs: 30,
        };

        // Should build without error
        let client = HttpClient::new(config);
        assert!(client.is_ok());
    }
}
