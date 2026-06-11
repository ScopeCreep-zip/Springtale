//! Shared serde helpers for connector configuration structs.
//!
//! Connectors that store credentials in `SecretBox<String>` can use these
//! deserialization helpers in `#[serde(deserialize_with = "...")]` attributes
//! instead of duplicating the boilerplate in every connector crate.

use secrecy::SecretBox;
use serde::Deserialize;

/// Deserialize a `String` value into a `SecretBox<String>`.
///
/// Usage:
/// ```ignore
/// #[serde(deserialize_with = "springtale_connector::config::deserialize_secret")]
/// pub token: SecretBox<String>,
/// ```
pub fn deserialize_secret<'de, D>(deserializer: D) -> Result<SecretBox<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(SecretBox::new(Box::new(s)))
}

/// Deserialize an optional `String` value into `Option<SecretBox<String>>`.
///
/// Usage:
/// ```ignore
/// #[serde(default, deserialize_with = "springtale_connector::config::deserialize_secret_option")]
/// pub webhook_secret: Option<SecretBox<String>>,
/// ```
pub fn deserialize_secret_option<'de, D>(
    deserializer: D,
) -> Result<Option<SecretBox<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.map(|s| SecretBox::new(Box::new(s))))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // Test fixture — never crosses the IPC boundary, so no Type
    // derive. (The production AI adapter configs that DO cross IPC
    // are tagged `#[specta(type = String)]` on their SecretBox fields
    // — see springtale-ai/src/{openai,anthropic}/adapter.rs.)
    #[derive(Deserialize)]
    struct TestConfig {
        #[serde(deserialize_with = "deserialize_secret")]
        token: SecretBox<String>,
        #[serde(default, deserialize_with = "deserialize_secret_option")]
        optional_secret: Option<SecretBox<String>>,
    }

    #[test]
    fn test_deserialize_secret_from_toml() {
        let toml_str = r#"token = "my_secret_value""#;
        let config: TestConfig = toml::from_str(toml_str).unwrap();
        assert!(springtale_crypto::secret_use::secret_eq_str(
            &config.token,
            "my_secret_value"
        ));
        assert!(config.optional_secret.is_none());
    }

    #[test]
    fn test_deserialize_secret_option_present() {
        let toml_str = r#"
token = "tok"
optional_secret = "opt_secret"
"#;
        let config: TestConfig = toml::from_str(toml_str).unwrap();
        assert!(springtale_crypto::secret_use::secret_eq_str(
            &config.optional_secret.unwrap(),
            "opt_secret"
        ));
    }

    #[test]
    fn test_deserialize_secret_option_absent() {
        let toml_str = r#"token = "tok""#;
        let config: TestConfig = toml::from_str(toml_str).unwrap();
        assert!(config.optional_secret.is_none());
    }
}
