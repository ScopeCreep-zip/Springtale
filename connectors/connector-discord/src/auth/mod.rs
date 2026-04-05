/// Bot token format validation.
///
/// Discord bot tokens follow the format: `{base64_bot_id}.{base64_timestamp}.{base64_hmac}`
/// Three segments separated by periods. We validate the structure without
/// decoding (the actual auth happens when twilight connects to the gateway).
pub fn validate_bot_token(token: &str) -> Result<(), crate::error::DiscordError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 3 {
        return Err(crate::error::DiscordError::AuthFailed(
            "bot token must contain at least 3 period-separated segments \
             (format: base64.base64.base64)"
                .into(),
        ));
    }
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return Err(crate::error::DiscordError::AuthFailed(format!(
                "bot token segment {i} is empty"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_token() {
        assert!(validate_bot_token("NDcyNTk2MDcw.Dk5MTI.abc123def456").is_ok());
    }

    #[test]
    fn test_validate_invalid_no_periods() {
        assert!(validate_bot_token("noperiodshere").is_err());
    }

    #[test]
    fn test_validate_invalid_too_few_segments() {
        assert!(validate_bot_token("only.two").is_err());
    }

    #[test]
    fn test_validate_invalid_empty_segment() {
        assert!(validate_bot_token("first..third").is_err());
        assert!(validate_bot_token(".second.third").is_err());
    }
}
