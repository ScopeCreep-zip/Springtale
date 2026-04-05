/// Validate the signal-cli daemon URL.
///
/// signal-cli daemon runs locally — HTTP is acceptable for localhost.
/// HTTPS is not required because the daemon is on the same machine.
pub fn validate_daemon_url(url: &str) -> Result<(), crate::error::SignalError> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(crate::error::SignalError::ConnectionFailed(
            "daemon_url must start with http:// or https://".into(),
        ));
    }
    if url.len() < 10 {
        return Err(crate::error::SignalError::ConnectionFailed(
            "daemon_url too short".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_http_url() {
        assert!(validate_daemon_url("http://localhost:8080").is_ok());
    }

    #[test]
    fn test_valid_https_url() {
        assert!(validate_daemon_url("https://signal.local:9000").is_ok());
    }

    #[test]
    fn test_invalid_scheme() {
        assert!(validate_daemon_url("ftp://localhost:8080").is_err());
    }

    #[test]
    fn test_too_short() {
        assert!(validate_daemon_url("http://x").is_err());
    }
}
