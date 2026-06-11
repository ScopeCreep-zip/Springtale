#![no_main]

use libfuzzer_sys::fuzz_target;

use std::path::{Path, PathBuf};

// Filesystem connector path canonicalization fuzz.
//
// CISA Secure-by-Design Path Traversal alert: any user-influenced filesystem
// path must canonicalize against a fixed base scope and refuse traversal.
// This target exercises that boundary against arbitrary attacker bytes —
// the canonicalizer must either return a path inside the base scope or
// an error, never escape and never panic.

fn canonicalize_within<'a>(base: &'a Path, candidate: &str) -> Option<PathBuf> {
    let combined = base.join(candidate);
    let canon = combined.canonicalize().ok()?;
    if canon.starts_with(base) {
        Some(canon)
    } else {
        None
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Use a fixed in-process base so we don't hammer the filesystem.
        let base = Path::new("/tmp/springtale-fuzz-base");
        let _ = canonicalize_within(base, s);
    }
});
