//! The `Action::Chain` arm — sub-step expansion inside one shared
//! [`ChainContext`].

use std::sync::Arc;

use springtale_cooperation::execution::ExecutionContext;
use springtale_core::rule::action::Action;
use springtale_core::rule::{ChainContext, ChainError};
use springtale_sentinel::Sentinel;

use super::step::run_step;
use crate::cooperation::CapabilityBridge;

/// Run every sub-step of a chain action against the shared context.
///
/// Records no wrapper step of its own: each sub-step lands in
/// `chain.steps` as it runs. `ChainError::Suppressed` from a nested dedupe
/// propagates unchanged so the outer caller can end the execution `empty`.
pub(super) async fn run_chain_steps(
    steps: &[Action],
    bridge: &CapabilityBridge,
    sentinel: &Arc<Sentinel>,
    execution: &ExecutionContext,
    chain: &mut ChainContext,
    depth: u32,
) -> Result<(), ChainError> {
    let new_depth = depth + 1;
    if new_depth > springtale_core::rule::action::MAX_CHAIN_DEPTH {
        return Err(ChainError::DepthExceeded {
            depth: new_depth,
            max: springtale_core::rule::action::MAX_CHAIN_DEPTH,
        });
    }
    // Chain expands transparently — each sub-step is recorded
    // as its own StepOutput in the shared ChainContext. The
    // Chain action itself doesn't produce a wrapper step.
    for (i, step) in steps.iter().enumerate() {
        match run_step(step, bridge, sentinel, execution, chain, new_depth).await {
            Ok(()) => {}
            Err(ChainError::Suppressed) => {
                // A nested dedupe step suppressed the chain —
                // propagate cleanly so the outer caller can
                // mark execution status `empty`.
                return Err(ChainError::Suppressed);
            }
            Err(e) => {
                tracing::warn!(step = i, error = %e, "chain step failed");
                return Err(e);
            }
        }
    }
    // Chain returns without recording its own StepOutput —
    // sub-steps are already in chain.steps.
    //
    // Skip the post-step alias refresh path below: we already
    // returned the sub-steps individually.
    Ok(())
}
