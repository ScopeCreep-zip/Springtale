//! Step 6 — broadcast the updated `FormationContext` (intent, momentum,
//! pacing phase) to every member subscribed to the watch channel.
//!
//! Wraps `Formation::broadcast_context` to keep the per-tick pipeline
//! uniform. Step bodies are typed `pub fn run` entrypoints even when the
//! work is a single call so swapping or mocking the publish layer is a
//! one-file change.

use crate::cooperation::formation::Formation;

pub fn run(formation: &Formation) {
    formation.broadcast_context();
}
