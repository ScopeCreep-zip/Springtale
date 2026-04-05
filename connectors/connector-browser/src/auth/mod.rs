/// Validate that a URL's domain is in the allow-list.
///
/// Extracts the host from the URL and checks against allowed domains.
/// No wildcards — exact domain match only (per security.md).
pub fn validate_domain(
    url: &str,
    allowed_domains: &[String],
) -> Result<(), crate::error::BrowserError> {
    let host = extract_host(url).ok_or_else(|| {
        crate::error::BrowserError::InvalidInput(format!("cannot extract host from URL: {url}"))
    })?;

    if !allowed_domains.iter().any(|d| d == &host) {
        return Err(crate::error::BrowserError::DomainNotAllowed(format!(
            "domain '{host}' is not in the allow-list"
        )));
    }

    Ok(())
}

/// Extract the host (domain) from a URL.
fn extract_host(url: &str) -> Option<String> {
    // Simple extraction: skip scheme, take host before port/path
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = without_scheme.split('/').next()?.split(':').next()?;
    Some(host.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_domain() {
        let domains = vec!["example.com".to_owned()];
        assert!(validate_domain("https://example.com/page", &domains).is_ok());
    }

    #[test]
    fn test_invalid_domain() {
        let domains = vec!["example.com".to_owned()];
        assert!(validate_domain("https://evil.com/page", &domains).is_err());
    }

    #[test]
    fn test_domain_with_port() {
        let domains = vec!["localhost".to_owned()];
        assert!(validate_domain("http://localhost:8080/path", &domains).is_ok());
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(
            extract_host("https://example.com/page"),
            Some("example.com".to_owned())
        );
        assert_eq!(
            extract_host("http://localhost:8080"),
            Some("localhost".to_owned())
        );
        assert_eq!(extract_host("ftp://bad"), None);
    }
}
