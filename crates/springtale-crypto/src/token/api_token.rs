//! API token derivation from vault passphrase.
//!
//! Uses HMAC-SHA256 to derive a deterministic authentication token hash
//! from the user's vault passphrase. The management API compares incoming
//! Bearer tokens against this hash.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Derive an API authentication token hash from a passphrase.
///
/// Computes `HMAC-SHA256(passphrase, "springtale-api-token")` and returns
/// the 32-byte digest. HMAC-SHA256 accepts any key size, so this function
/// is infallible.
pub fn derive_api_token_hash(passphrase: &[u8]) -> [u8; 32] {
    // HMAC-SHA256 accepts any key size — this cannot fail.
    #[allow(clippy::expect_used)]
    let mut mac = HmacSha256::new_from_slice(passphrase)
        .expect("HMAC-SHA256 accepts any key size");
    mac.update(b"springtale-api-token");
    let result = mac.finalize();
    let bytes = result.into_bytes();

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    hash
}
