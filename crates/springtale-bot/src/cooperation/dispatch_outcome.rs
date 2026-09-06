//! What one member's dispatch produced, and the slot that carries a
//! dispatch across beats (plan 1.8 / 0.3).
//!
//! `ExecuteOutcome` is the executor's return type: what the report says,
//! what `post` has to write back, and — per 0.3 — the `ActionState` the
//! member reached, so a dispatch that outlived its beat is reported as
//! `Requested`, never as success. `PendingDispatch` is that dispatch: the
//! spawned task keeps running; the member reports `Requested` each beat
//! until `act` collects the finished handle (Left 4 Dead: a teammate
//! mid-action is not the team's problem until it is).

use std::time::Instant;

use tokio::task::JoinHandle;

use springtale_cooperation::TickId;
use springtale_cooperation::action::SubTask;
use springtale_cooperation::action_state::ActionState;
use springtale_cooperation::cadence::ActionDescriptor;
use springtale_core::rule::action::Action;

/// Alignment reported while a dispatch is still running past its beat:
/// the member is working, not idle, but nothing has succeeded yet.
pub const REQUESTED_ALIGNMENT: f32 = 0.8;

pub struct ExecuteOutcome {
    pub action_descriptor: Option<ActionDescriptor>,
    pub alignment: f32,
    /// `Some(task)` when a destructive action needs a consensus vote —
    /// the pipeline fires `consensus.propose` after the gather phase.
    pub consensus_task: Option<SubTask>,
    /// State the member's action reached this beat (0.3). `Init` when no
    /// action was dispatched; `Requested` for a dispatch carried over.
    pub state: ActionState,
    /// Wall-clock time the connector call took; `0` when nothing ran.
    pub duration_ms: u64,
    /// Present only when a connector actually ran — what `post` writes
    /// back (result row, audit write, stigmergy deposit).
    pub dispatched: Option<Dispatched>,
}

impl ExecuteOutcome {
    /// Nothing reached a connector: yield, observe, suggest, a deferred
    /// or lost claim, a consensus proposal.
    pub fn settled(action_descriptor: Option<ActionDescriptor>, alignment: f32) -> Self {
        Self {
            action_descriptor,
            alignment,
            consensus_task: None,
            state: ActionState::Init,
            duration_ms: 0,
            dispatched: None,
        }
    }

    /// A dispatch still running past the beat (0.3): reported `Requested`.
    pub fn requested(descriptor: ActionDescriptor) -> Self {
        Self {
            action_descriptor: Some(descriptor),
            alignment: REQUESTED_ALIGNMENT,
            consensus_task: None,
            state: ActionState::Requested,
            duration_ms: 0,
            dispatched: None,
        }
    }
}

/// A connector call that ran to completion (success or failure).
pub struct Dispatched {
    pub task: SubTask,
    pub action: Action,
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

/// A dispatch that outlived its beat.
pub struct PendingDispatch {
    pub handle: JoinHandle<ExecuteOutcome>,
    pub task_id: uuid::Uuid,
    pub descriptor: ActionDescriptor,
    /// When the dispatch was carried over; `supervision` compares this
    /// with `constraints.timeout` to mark the member `Suspect`.
    pub since: Instant,
    pub since_tick: TickId,
}

/// `FormationMember.pending`. Cloning a member yields an empty slot: a
/// copy of a member does not own the in-flight task.
#[derive(Default)]
pub struct PendingSlot(Option<PendingDispatch>);

impl PendingSlot {
    pub fn set(&mut self, pending: PendingDispatch) {
        self.0 = Some(pending);
    }

    pub fn take(&mut self) -> Option<PendingDispatch> {
        self.0.take()
    }

    pub fn get(&self) -> Option<&PendingDispatch> {
        self.0.as_ref()
    }

    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }
}

impl Clone for PendingSlot {
    fn clone(&self) -> Self {
        Self(None)
    }
}
