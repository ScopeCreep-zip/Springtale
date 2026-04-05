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
