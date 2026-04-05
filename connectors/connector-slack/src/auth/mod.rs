/// Validate Slack token format.
///
/// Bot tokens start with `xoxb-`. App-level tokens start with `xapp-`.
pub fn validate_bot_token(token: &str) -> Result<(), crate::error::SlackError> {
    if !token.starts_with("xoxb-") {
        return Err(crate::error::SlackError::AuthFailed(
            "bot token must start with 'xoxb-'".into(),
        ));
    }
    if token.len() < 10 {
        return Err(crate::error::SlackError::AuthFailed(
            "bot token too short".into(),
        ));
    }
    Ok(())
}

/// Validate app-level token format for Socket Mode.
pub fn validate_app_token(token: &str) -> Result<(), crate::error::SlackError> {
    if !token.starts_with("xapp-") {
        return Err(crate::error::SlackError::AuthFailed(
            "app token must start with 'xapp-' (Socket Mode requires an app-level token)".into(),
        ));
    }
    if token.len() < 10 {
        return Err(crate::error::SlackError::AuthFailed(
            "app token too short".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_bot_token() {
        assert!(validate_bot_token("xoxb-123456-abcdefgh").is_ok());
    }

    #[test]
    fn test_invalid_bot_token_prefix() {
        assert!(validate_bot_token("xoxp-user-token").is_err());
        assert!(validate_bot_token("Bearer token").is_err());
    }

    #[test]
    fn test_bot_token_too_short() {
        assert!(validate_bot_token("xoxb-").is_err());
    }

    #[test]
    fn test_valid_app_token() {
        assert!(validate_app_token("xapp-1-A0123-abcdefgh").is_ok());
    }

    #[test]
    fn test_invalid_app_token_prefix() {
        assert!(validate_app_token("xoxb-wrong-type").is_err());
    }

    #[test]
    fn test_app_token_too_short() {
        assert!(validate_app_token("xapp-").is_err());
    }
}
