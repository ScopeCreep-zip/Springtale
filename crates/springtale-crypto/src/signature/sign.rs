use crate::identity::keypair::Keypair;

/// Sign arbitrary bytes with the keypair's Ed25519 signing key.
pub fn sign_bytes(keypair: &Keypair, data: &[u8]) -> ed25519_dalek::Signature {
    keypair.sign(data)
}

/// Sign canonical JSON: sort keys deterministically, then sign the bytes.
///
/// This ensures that the same JSON object always produces the same signature
/// regardless of key ordering in the source.
pub fn sign_canonical_json(
    keypair: &Keypair,
    value: &serde_json::Value,
) -> Result<ed25519_dalek::Signature, crate::error::CryptoError> {
    let canonical = canonical_json(value)?;
    Ok(sign_bytes(keypair, canonical.as_bytes()))
}

/// Produce canonical JSON: keys sorted recursively, no trailing whitespace.
pub fn canonical_json(value: &serde_json::Value) -> Result<String, crate::error::CryptoError> {
    let sorted = sort_json_keys(value);
    serde_json::to_string(&sorted)
        .map_err(|e| crate::error::CryptoError::Serialization(e.to_string()))
}

/// Recursively sort all object keys in a JSON value.
fn sort_json_keys(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                if let Some(val) = map.get(key) {
                    sorted.insert(key.clone(), sort_json_keys(val));
                }
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(sort_json_keys).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_canonical_json_sorts_keys() {
        let val = json!({"z": 1, "a": 2, "m": 3});
        let canonical = canonical_json(&val);
        assert!(canonical.is_ok());
        assert_eq!(canonical.ok().as_deref(), Some(r#"{"a":2,"m":3,"z":1}"#));
    }

    #[test]
    fn test_canonical_json_nested() {
        let val = json!({"b": {"z": 1, "a": 2}, "a": 0});
        let canonical = canonical_json(&val);
        assert!(canonical.is_ok());
        assert_eq!(
            canonical.ok().as_deref(),
            Some(r#"{"a":0,"b":{"a":2,"z":1}}"#)
        );
    }

    #[test]
    fn test_sign_and_verify_canonical_json() {
        let keypair = Keypair::generate();
        assert!(keypair.is_ok());
        let keypair = keypair.ok();

        let value = json!({"name": "test-connector", "version": "1.0"});

        let sig = keypair
            .as_ref()
            .and_then(|kp| sign_canonical_json(kp, &value).ok());
        assert!(sig.is_some());

        // Verify with the public key
        let canonical = canonical_json(&value);
        assert!(canonical.is_ok());

        use ed25519_dalek::Verifier;
        let result = keypair.as_ref().and_then(|kp| {
            canonical.as_ref().ok().and_then(|c| {
                sig.as_ref()
                    .map(|s| kp.verifying_key().verify(c.as_bytes(), s))
            })
        });
        assert!(result.is_some_and(|r| r.is_ok()));
    }
}
