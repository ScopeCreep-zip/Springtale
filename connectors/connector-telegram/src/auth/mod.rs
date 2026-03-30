/// Bot token validation.
///
/// Telegram bot tokens follow the format: `{bot_id}:{secret}`.
/// The bot_id is a numeric identifier, and the secret is an alphanumeric string.
pub fn validate_bot_token(token: &str) -> Result<(), crate::error::TelegramError> {
    if !token.contains(':') {
        return Err(crate::error::TelegramError::InvalidConfig(
            "bot token must contain a colon separator (format: 123456:ABC-DEF)".into(),
        ));
    }
    let parts: Vec<&str> = token.splitn(2, ':').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(crate::error::TelegramError::InvalidConfig(
            "invalid bot token format".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_token() {
        assert!(validate_bot_token("123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11").is_ok());
    }

    #[test]
    fn test_invalid_no_colon() {
        assert!(validate_bot_token("nocolonhere").is_err());
    }

    #[test]
    fn test_invalid_empty_parts() {
        assert!(validate_bot_token(":secret").is_err());
        assert!(validate_bot_token("id:").is_err());
    }
}
