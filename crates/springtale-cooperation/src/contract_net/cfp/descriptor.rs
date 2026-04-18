use std::time::Duration;

use uuid::Uuid;

use crate::action::SubTask;
use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;

use crate::contract_net::types::CallForProposals;

/// Construct a CFP for a given task. Kept here (not on `CallForProposals`
/// itself) so future variants — e.g. iterated CNP with scoring hints — each
/// get their own constructor file without bloating the types module.
pub fn for_task(
    initiator: AgentId,
    task: SubTask,
    deadline: Duration,
    required_capability: Option<CapabilityDecl>,
) -> CallForProposals {
    CallForProposals {
        id: Uuid::new_v4(),
        initiator,
        task,
        deadline,
        required_capability,
        scoring_hint: None,
    }
}
