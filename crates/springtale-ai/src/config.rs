use secrecy::SecretBox;
use serde::Deserialize;

/// Deserialize a plain string into `SecretBox<String>`.
///
/// Used for API key fields in adapter configs. The secrecy crate's serde
/// feature does not provide Deserialize for SecretBox by default —
/// this explicit deserializer ensures secrets are wrapped at the exact
/// deserialization boundary.
pub fn deserialize_secret<'de, D>(deserializer: D) -> Result<SecretBox<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(SecretBox::new(Box::new(s)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    //! Direct unit test for `deserialize_secret` — Phase-7 audit
    //! Finding E. Indirectly exercised by every adapter config test
    //! that walks a `Secret<String>` field, but the audit flagged
    //! the absence of a focused round-trip that proves:
    //!
    //!   1. JSON + TOML inputs both flow through cleanly.
    //!   2. The wrapped value is recoverable via `expose_secret`.
    //!   3. The redacted Debug printing never leaks the cleartext.

    use super::*;
    use secrecy::ExposeSecret;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Wrapper {
        #[serde(deserialize_with = "deserialize_secret")]
        api_key: SecretBox<String>,
    }

    #[test]
    fn deserialize_secret_round_trips_json_value() {
        let w: Wrapper = serde_json::from_str(r#"{"api_key":"sk-test-abc-123"}"#).unwrap();
        // SECURITY: expose needed in test to compare round-tripped value
        assert_eq!(w.api_key.expose_secret(), "sk-test-abc-123");
    }

    #[test]
    fn deserialize_secret_round_trips_toml_value() {
        let w: Wrapper = toml::from_str(r#"api_key = "tok_live_xyz""#).unwrap();
        // SECURITY: expose needed in test to compare round-tripped value
        assert_eq!(w.api_key.expose_secret(), "tok_live_xyz");
    }

    #[test]
    fn deserialize_secret_debug_does_not_leak_cleartext() {
        let w: Wrapper = serde_json::from_str(r#"{"api_key":"super-sensitive"}"#).unwrap();
        let debug_repr = format!("{:?}", w.api_key);
        assert!(
            !debug_repr.contains("super-sensitive"),
            "SecretBox Debug leaked cleartext: {debug_repr}"
        );
    }

    #[test]
    fn deserialize_secret_rejects_non_string_input() {
        // serde-json drives an i64 in here; `String::deserialize`
        // refuses, which the wrapper propagates as an Err.
        let r: Result<Wrapper, _> = serde_json::from_str(r#"{"api_key":42}"#);
        assert!(r.is_err(), "expected non-string input to error");
    }

    #[test]
    fn deserialize_secret_preserves_empty_string() {
        // An empty secret is still a valid secret value (e.g.
        // an explicit "no token" placeholder). The deserializer
        // must NOT drop or coerce empties.
        let w: Wrapper = serde_json::from_str(r#"{"api_key":""}"#).unwrap();
        // SECURITY: expose needed in test to assert empty preservation
        assert_eq!(w.api_key.expose_secret(), "");
    }
}
