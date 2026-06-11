//! Agent tick composition (plan §A2 reference code, lines 738–770).
//!
//! `AgentLoop<R, S, B, N>` holds the four trait-object generics from the
//! plan: `TaskRouter`, `SurfaceSensor`, `StateSubscriber`, `Bidder`.
//! `tick()` composes the per-step files in canonical layer order with the
//! first-non-None early exit pattern.
//!
//! The `B: StateSubscriber` field is wrapped in `tokio::sync::Mutex`
//! because `try_recv` needs `&mut self`; runner tasks hold their own
//! subscription so contention is single-task in practice.

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::agent::context::AgentContext;
use crate::agent::result::AgentTickResult;
use crate::agent::step;
use crate::awareness::LocalAwareness;
use crate::contract_net::trait_::Bidder;
use crate::contract_net::types::{Bid, CallForProposals};
use crate::dissemination::trait_::StateSubscriber;
use crate::routing::trait_::TaskRouter;
use crate::stigmergy::trait_::SurfaceSensor;

pub struct AgentLoop<R, S, B, N>
where
    R: TaskRouter,
    S: SurfaceSensor,
    B: StateSubscriber,
    N: Bidder,
{
    pub router: Arc<R>,
    pub sensor: Arc<S>,
    pub bus: Arc<Mutex<B>>,
    pub bidder: Arc<N>,
}

impl<R, S, B, N> AgentLoop<R, S, B, N>
where
    R: TaskRouter,
    S: SurfaceSensor,
    B: StateSubscriber,
    N: Bidder,
{
    /// One agent tick. Layer order matches plan §A2:
    /// L0 sense → L3 inbox → L2 react (awareness only) → L1 scan.
    /// CFP responses fire reactively in the runner via `respond_to_cfp`.
    pub async fn tick(
        &self,
        ctx: &AgentContext<'_>,
        awareness: &mut LocalAwareness,
    ) -> AgentTickResult {
        if let Some(r) = step::sense::run(&*self.sensor, awareness, ctx) {
            return r;
        }
        if let Some(r) = step::inbox::run(&*self.router, ctx).await {
            return r;
        }
        let mut bus = self.bus.lock().await;
        step::react::run(&mut *bus, awareness, ctx.momentum.tier);
        drop(bus);
        if let Some(r) = step::scan::run(&*self.router, ctx).await {
            return r;
        }
        AgentTickResult::idle(ctx.agent_id)
    }

    /// Reactive CFP path — runner calls this when a CFP arrives on the
    /// L4 channel. Decoupled from `tick` so per-tick latency isn't
    /// gated by CFP processing.
    pub async fn on_cfp(&self, cfp: &CallForProposals, ctx: &AgentContext<'_>) -> Option<Bid> {
        step::respond_cfp::run(&*self.bidder, cfp, ctx).await
    }
}
