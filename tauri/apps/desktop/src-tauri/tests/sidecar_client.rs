//! Plan 2.1 — the desktop shell is a sidecar client of `springtaled`.
//!
//! The regression this guards against is re-linking the daemon's crates
//! into the shell: the moment `springtale-bot`, `springtale-runtime` or
//! `springtale-store` is a dependency again, the desktop binary is capable
//! of running a second bot loop, scheduler and store against the same
//! database, which is what findings 3, 8 and 10 were about.

use std::path::PathBuf;

/// Crates that must never be linked into the desktop shell again.
const FORBIDDEN: &[&str] = &["springtale-bot", "springtale-runtime", "springtale-store"];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn test_cargo_toml_has_no_daemon_crates() {
    let manifest = std::fs::read_to_string(manifest_dir().join("Cargo.toml"))
        .expect("src-tauri/Cargo.toml is readable");
    for crate_name in FORBIDDEN {
        assert!(
            !manifest.contains(&format!("{crate_name} =")),
            "{crate_name} is a dependency of springtale-desktop again — \
             the shell must reach the daemon over HTTP, not link its crates"
        );
    }
}

#[test]
fn test_cargo_lock_has_no_daemon_crates() {
    let lock_path = manifest_dir().join("Cargo.lock");
    let lock = std::fs::read_to_string(&lock_path).expect("src-tauri/Cargo.lock is readable");
    for crate_name in FORBIDDEN {
        assert!(
            !lock.contains(&format!("name = \"{crate_name}\"")),
            "{crate_name} is in the desktop Cargo.lock — it is reachable \
             transitively from one of the remaining dependencies"
        );
    }
}
