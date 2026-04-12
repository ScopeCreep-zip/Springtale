//! Memory locking — prevent key material from being swapped to disk.
//!
//! For IPV survivors and activists whose devices may be physically seized,
//! key material in swap/pagefile is a real threat vector. `mlock` pins
//! memory pages into physical RAM so the OS cannot swap them out.
//!
//! Uses `memsec` for cross-platform support (Unix `mlock`, Windows `VirtualLock`).
//!
//! Platform notes:
//! - Linux: works up to `RLIMIT_MEMLOCK` (typically 64-256KB per process)
//! - macOS: may silently fail for unprivileged processes — non-fatal
//! - Windows: maps to `VirtualLock`, requires `SE_LOCK_MEMORY_PRIVILEGE` for large allocations

use secrecy::{ExposeSecret, SecretBox};

/// Lock a 32-byte secret key into physical RAM.
///
/// Returns `true` if the OS accepted the lock, `false` if it refused.
/// Failure is logged but non-fatal — mlock is defense-in-depth, not
/// a hard requirement. The key is still protected by `SecretBox`
/// (zeroize-on-drop) regardless.
pub fn lock_key(secret: &SecretBox<[u8; 32]>) -> bool {
    // SECURITY: expose needed for mlock pointer to pin key in physical RAM
    let ptr = secret.expose_secret().as_ptr();
    // SAFETY: `ptr` points to a valid heap-allocated [u8; 32] owned by
    // SecretBox. Box<[u8; 32]> has a stable address (no reallocation).
    // memsec::mlock calls libc::mlock (Unix) or VirtualLock (Windows)
    // to pin the containing page(s) into physical RAM.
    #[allow(unsafe_code)]
    let locked = unsafe { memsec::mlock(ptr as *mut u8, 32) };

    // Exclude from core dumps on Linux — prevents key recovery from crash dumps.
    // IPV survivors' devices may be forensically examined; core dumps must not
    // contain key material. macOS lacks MADV_DONTDUMP; mlock alone is sufficient there.
    #[cfg(target_os = "linux")]
    if locked {
        // SAFETY: same pointer as above, valid heap allocation owned by SecretBox.
        // MADV_DONTDUMP advises the kernel to exclude these pages from core dumps.
        #[allow(unsafe_code)]
        unsafe {
            libc::madvise(ptr as *mut libc::c_void, 32, libc::MADV_DONTDUMP);
        }
    }

    locked
}

/// Unlock key material before dropping. Call before SecretBox is dropped
/// so the OS can reclaim the page lock.
pub fn unlock_key(secret: &SecretBox<[u8; 32]>) -> bool {
    // SECURITY: expose needed for munlock pointer to release page lock before drop
    let ptr = secret.expose_secret().as_ptr();
    // SAFETY: same invariants as lock_key — valid heap pointer, stable address.
    #[allow(unsafe_code)]
    unsafe {
        memsec::munlock(ptr as *mut u8, 32)
    }
}
