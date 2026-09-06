//! The `Action::Extract` arm — the extraction ladder's step wrapper.

use springtale_cooperation::execution::ExecutionContext;
use springtale_core::rule::template_resolve::resolve_chain_value;
use springtale_core::rule::{ChainContext, ChainError, StepOutput};

use crate::cooperation::CapabilityBridge;
/// Resolve the extraction source against the chain and run the ladder.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_extract_step(
    source: &str,
    extract_kind: &springtale_core::rule::action::ExtractKind,
    bridge: &CapabilityBridge,
    execution: &ExecutionContext,
    chain: &mut ChainContext,
    run_id: &str,
    kind: &'static str,
    started: std::time::Instant,
) -> Result<StepOutput, ChainError> {
    // Resolve `source` as a path against the chain — e.g.
    // `"last_connector_output.body"` → the HTTP body string,
    // or `"trigger.payload"` → the trigger event JSON.
    let resolved_source = resolve_chain_value(
        &serde_json::Value::String(format!("${{{source}}}")),
        chain,
        Some(run_id),
    );

    // The AI adapter for LlmSchema extraction. We pass it
    // through opt-in — Phase A only fires non-LLM tiers;
    // Phase B activates LlmSchema and the adapter is read.
    let adapter_arc = bridge.ai_adapter_for(execution).await;
    let ai_ref: Option<&dyn springtale_ai::AiAdapter> = Some(&*adapter_arc);

    let extracted = crate::extraction::extract(&resolved_source, extract_kind, ai_ref).await;
    match extracted {
        Ok(value) => Ok(StepOutput {
            index: chain.next_step_index(),
            kind: kind.into(),
            name: None,
            output: value,
            duration_ms: started.elapsed().as_millis() as u64,
            error: None,
        }),
        Err(e) => Err(ChainError::StepFailed {
            index: chain.next_step_index(),
            kind: kind.into(),
            message: e.to_string(),
        }),
    }
}
