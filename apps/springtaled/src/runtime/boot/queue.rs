use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

use springtale_scheduler::queue::consumer::JobConsumer;
use springtale_scheduler::queue::producer::JobProducer;

/// Initialize job queue (producer + consumer with sentinel dispatch).
///
/// Spawns the consumer as a background task and returns the producer for
/// use by the event loop.
pub(super) async fn init_job_queue(
    runtime: &springtale_runtime::RuntimeState,
) -> Result<Arc<JobProducer>> {
    let (job_tx, job_rx) = mpsc::channel(100);
    let producer = Arc::new(JobProducer::new(job_tx));
    let mut consumer = JobConsumer::new(job_rx, 4);

    // Install action dispatcher as the job handler. Routes through the
    // shared `CapabilityBridge` on `RuntimeState` so queued jobs and
    // tick-driven dispatches share one enforcement point (§16 / §6.10).
    let dispatch_bridge = runtime.capability_bridge.clone();
    let dispatch_sentinel = runtime.sentinel.clone();
    consumer.set_handler(std::sync::Arc::new(move |job| {
        let bridge = dispatch_bridge.clone();
        let sent = dispatch_sentinel.clone();
        Box::pin(async move {
            let action: springtale_core::rule::action::Action = serde_json::from_value(job.payload)
                .map_err(|e| format!("failed to deserialize action: {e}"))?;

            // Sentinel evaluation + action dispatch in shared layer
            crate::dispatch::dispatch_action(&action, &bridge, &sent)
                .await
                .map(|_| ())
        })
    }));

    // Spawn consumer as background task
    tokio::spawn(async move {
        if let Err(e) = consumer.run().await {
            tracing::error!(error = %e, "job consumer error");
        }
    });
    tracing::info!("job queue started (concurrency: 4)");

    Ok(producer)
}
