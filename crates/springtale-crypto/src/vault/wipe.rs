use std::fs::File;
use std::io::Write;
use std::path::Path;

use rand::RngCore;

use crate::error::CryptoError;

/// Securely overwrite a vault file with random bytes, then delete it.
///
/// Single-pass random overwrite is sufficient per NIST 800-88 standards.
/// After overwriting, the file is synced to disk and deleted.
///
/// IMPORTANT: On SSDs with wear leveling, filesystem-level overwrites
/// cannot guarantee old physical blocks are erased. The vault's
/// XChaCha20-Poly1305 encryption is the primary defense — this wipe
/// is defense-in-depth for HDDs and to destroy plaintext structure.
///
/// For complete SSD erasure, use the manufacturer's Secure Erase command
/// (ATA Secure Erase or NVMe Secure Erase) on the entire drive.
pub fn wipe_vault_file(path: &Path) -> Result<(), CryptoError> {
    if !path.exists() {
        return Ok(()); // Nothing to wipe
    }

    let file_size = std::fs::metadata(path)?.len() as usize;

    // Overwrite with random bytes in 4KB chunks
    let mut file = File::options().write(true).open(path)?;
    let mut rng = rand::thread_rng();
    let mut buf = [0u8; 4096];
    let mut remaining = file_size;

    while remaining > 0 {
        rng.fill_bytes(&mut buf);
        let to_write = remaining.min(buf.len());
        file.write_all(&buf[..to_write])?;
        remaining -= to_write;
    }

    // Flush to disk before deleting
    file.sync_all()?;
    drop(file);

    // Delete the overwritten file
    std::fs::remove_file(path)?;

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_wipe_vault_file_destroys_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_vault.bin");

        // Write recognizable data
        std::fs::write(&path, b"SECRET_VAULT_DATA_12345").unwrap();
        assert!(path.exists());

        // Wipe
        wipe_vault_file(&path).unwrap();

        // File should be gone
        assert!(!path.exists());
    }

    #[test]
    fn test_wipe_nonexistent_file_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.bin");

        // Should succeed without error
        assert!(wipe_vault_file(&path).is_ok());
    }

    #[test]
    fn test_wipe_completes_within_3_seconds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large_vault.bin");

        // Create a 1MB file (larger than any real vault)
        let data = vec![0xAA; 1024 * 1024];
        std::fs::write(&path, &data).unwrap();

        let start = std::time::Instant::now();
        wipe_vault_file(&path).unwrap();
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_secs() < 3,
            "wipe took {:?}, exceeds 3-second deadline",
            elapsed
        );
    }
}
