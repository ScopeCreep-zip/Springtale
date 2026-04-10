//! Guest-side memory allocator for result buffers.
//!
//! WASM linear memory is a contiguous byte array. The host writes
//! action data at known offsets (starting at 1024). The guest needs
//! to allocate space for result buffers above the host-written region.
//!
//! This uses a simple bump allocator starting at offset 65536 (page 1).
//! Each invocation resets the allocator since the host creates a fresh
//! Store per call.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Bump allocator for guest result buffers.
/// Starts at offset 65536 (second WASM page) to avoid host-written region.
static ALLOC_OFFSET: AtomicUsize = AtomicUsize::new(65536);

/// Allocate `size` bytes in guest linear memory.
///
/// Returns the offset (pointer) to the allocated region.
/// The allocator bumps forward — no free(). Each host invocation
/// creates a fresh Store, so memory is implicitly reclaimed.
pub fn alloc(size: usize) -> usize {
    let offset = ALLOC_OFFSET.fetch_add(size, Ordering::SeqCst);
    offset
}

/// Reset the allocator to the initial offset.
///
/// Called at the start of each `execute()` if the guest wants to
/// reuse memory across multiple calls within the same Store.
#[allow(dead_code)]
pub fn reset() {
    ALLOC_OFFSET.store(65536, Ordering::SeqCst);
}
