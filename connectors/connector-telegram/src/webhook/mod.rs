use crate::error::TelegramError;

/// Verify a Telegram webhook request by checking the secret_token header.
///
/// Telegram's webhook auth model: when you call `setWebhook` with a `secret_token`,
/// Telegram includes it as `X-Telegram-Bot-Api-Secret-Token` header in every
/// webhook POST. Verification is constant-time string comparison.
pub fn verify_webhook_secret(
    expected: &secrecy::SecretBox<String>,
    received_header: &str,
) -> Result<(), TelegramError> {
    if springtale_crypto::secret_use::secret_eq_str(expected, received_header) {
        Ok(())
    } else {
        Err(TelegramError::WebhookVerificationFailed)
    }
}

/// Determine trigger name from a Telegram update payload.
pub fn update_to_trigger(update: &serde_json::Value) -> Option<&'static str> {
    if let Some(message) = update.get("message") {
        if let Some(text) = message.get("text").and_then(|t| t.as_str())
            && text.starts_with('/')
        {
            return Some("command_received");
        }
        return Some("message_received");
    }
    if update.get("callback_query").is_some() {
        return Some("callback_query_received");
    }
    None
}

/// Parse `/command@botname args` into `("command", "args")`.
pub fn parse_command(text: &str) -> (String, String) {
    let without_slash = text.strip_prefix('/').unwrap_or(text);
    let mut parts = without_slash.splitn(2, ' ');
    let command_part = parts.next().unwrap_or("");
    let args = parts.next().unwrap_or("").to_owned();
    // Strip @botname suffix
    let command = command_part.split('@').next().unwrap_or("").to_owned();
    (command, args)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use secrecy::SecretBox;

    #[test]
    fn test_verify_valid_secret() {
        let expected = SecretBox::new(Box::new("my_secret_token".to_owned()));
        assert!(verify_webhook_secret(&expected, "my_secret_token").is_ok());
    }

    #[test]
    fn test_verify_invalid_secret() {
        let expected = SecretBox::new(Box::new("my_secret_token".to_owned()));
        assert!(verify_webhook_secret(&expected, "wrong_token").is_err());
    }

    #[test]
    fn test_verify_empty_secret() {
        let expected = SecretBox::new(Box::new("secret".to_owned()));
        assert!(verify_webhook_secret(&expected, "").is_err());
    }

    #[test]
    fn test_update_to_trigger_message() {
        let update = serde_json::json!({
            "update_id": 1,
            "message": { "message_id": 42, "text": "hello", "chat": { "id": 1 } }
        });
        assert_eq!(update_to_trigger(&update), Some("message_received"));
    }

    #[test]
    fn test_update_to_trigger_command() {
        let update = serde_json::json!({
            "update_id": 1,
            "message": { "message_id": 42, "text": "/start", "chat": { "id": 1 } }
        });
        assert_eq!(update_to_trigger(&update), Some("command_received"));
    }

    #[test]
    fn test_update_to_trigger_callback_query() {
        let update = serde_json::json!({ "update_id": 1, "callback_query": {} });
        assert_eq!(update_to_trigger(&update), Some("callback_query_received"));
    }

    #[test]
    fn test_update_to_trigger_unknown() {
        // A channel_post or other non-message, non-callback update → None
        let update = serde_json::json!({ "update_id": 1, "channel_post": {} });
        assert_eq!(update_to_trigger(&update), None);
    }

    #[test]
    fn test_parse_command_simple() {
        let (cmd, args) = parse_command("/start");
        assert_eq!(cmd, "start");
        assert_eq!(args, "");
    }

    #[test]
    fn test_parse_command_with_args() {
        let (cmd, args) = parse_command("/search tokyo weather");
        assert_eq!(cmd, "search");
        assert_eq!(args, "tokyo weather");
    }

    #[test]
    fn test_parse_command_with_botname() {
        let (cmd, args) = parse_command("/start@mybot hello");
        assert_eq!(cmd, "start");
        assert_eq!(args, "hello");
    }
}
