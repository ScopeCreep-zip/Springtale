//! Phase 4 tail: open the consensus votes this beat proposed (B7). Runs
//! after gather so the `&mut formation.members` borrow is released. The
//! voters list is every operational member; override tokens default to
//! 1 per agent (As Dusk Falls game default for high-stakes votes, §11).

use tokio::sync::broadcast;

use springtale_cooperation::CooperationEventEnvelope;
use springtale_cooperation::action::SubTask;
use springtale_cooperation::cadence::AgentId;
use springtale_cooperation::consensus::{DecisionDescriptor, DecisionSubject};
use springtale_cooperation::events::{self, CooperationEvent};

use crate::cooperation::formation::Formation;

pub fn propose_all(
    formation: &mut Formation,
    proposals: Vec<SubTask>,
    cooperation_tx: Option<&broadcast::Sender<CooperationEventEnvelope>>,
) {
    if proposals.is_empty() {
        return;
    }
    let voters: Vec<AgentId> = formation
        .members
        .iter()
        .filter(|m| m.is_operational())
        .map(|m| m.agent_id)
        .collect();
    let voter_count = voters.len() as u32;
    for task in proposals {
        let task_id = task.id;
        let id = formation.consensus.propose(
            DecisionDescriptor {
                description: format!(
                    "execute {}::{} (id={})",
                    task.target_connector.name, task.action_name, task.id
                ),
                options: vec!["approve".into(), "deny".into()],
                required_participants: voter_count,
                subject: DecisionSubject::DestructiveAction { task },
            },
            std::time::Duration::from_secs(5),
            &voters,
            1,
        );
        // B7 guard — while this entry exists, the executor won't
        // re-propose for the same task on subsequent beats.
        formation.awaiting_consensus.insert(task_id, id);
        tracing::info!(
            formation = %formation.id.0,
            vote_id = %id,
            task = %task_id,
            voters = voter_count,
            "consensus vote opened for destructive action"
        );
        // Phase H5: surface vote-opened so the formation event log shows
        // pending votes alongside the rest of the cooperation lifecycle.
        events::emit(
            cooperation_tx,
            CooperationEvent::ConsensusVoteOpened {
                formation_id: formation.id,
                vote_id: id,
                deadline_ms: 5_000,
            },
        );
    }
}
