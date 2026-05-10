use std::collections::VecDeque;
use std::sync::Mutex;

use dashmap::DashMap;

use crate::action::SubTask;
use crate::cadence::AgentId;

/// Per-agent FIFO of `SubTask`s directly assigned to them. Storing the full
/// SubTask (not just the id) keeps the L3 inbox self-contained — `poll`
/// returns work that's ready to execute without a separate blackboard
/// round-trip. Small hot path; a `Mutex<VecDeque>` is fine — contention is
/// per-agent, not global.
#[derive(Debug, Default)]
pub struct DirectInbox {
    per_agent: DashMap<AgentId, Mutex<VecDeque<SubTask>>>,
}

impl DirectInbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, agent: AgentId, task: SubTask) {
        let inbox = self.per_agent.entry(agent).or_default();
        if let Ok(mut q) = inbox.lock() {
            q.push_back(task);
        }
    }

    pub fn poll(&self, agent: AgentId) -> Option<SubTask> {
        self.per_agent
            .get(&agent)
            .and_then(|inbox| inbox.lock().ok().and_then(|mut q| q.pop_front()))
    }

    pub fn len(&self, agent: AgentId) -> usize {
        self.per_agent
            .get(&agent)
            .and_then(|inbox| inbox.lock().ok().map(|q| q.len()))
            .unwrap_or(0)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::capability::CapabilityDecl;

    fn make_subtask(name: &str) -> SubTask {
        SubTask {
            id: uuid::Uuid::new_v4(),
            target_connector: CapabilityDecl::new(name),
            action_name: "test".into(),
            params: serde_json::json!({}),
            priority: 1,
            assigned_to: None,
            description: name.into(),
        }
    }

    #[test]
    fn push_and_poll_fifo() {
        let inbox = DirectInbox::new();
        let agent = AgentId::new();
        let t1 = make_subtask("a");
        let t2 = make_subtask("b");
        let t1_id = t1.id;
        let t2_id = t2.id;
        inbox.push(agent, t1);
        inbox.push(agent, t2);
        assert_eq!(inbox.len(agent), 2);
        assert_eq!(inbox.poll(agent).map(|t| t.id), Some(t1_id));
        assert_eq!(inbox.poll(agent).map(|t| t.id), Some(t2_id));
        assert!(inbox.poll(agent).is_none());
    }

    #[test]
    fn different_agents_have_separate_inboxes() {
        let inbox = DirectInbox::new();
        let a = AgentId::new();
        let b = AgentId::new();
        inbox.push(a, make_subtask("x"));
        assert_eq!(inbox.len(a), 1);
        assert_eq!(inbox.len(b), 0);
    }

    #[test]
    fn poll_empty_returns_none() {
        let inbox = DirectInbox::new();
        assert!(inbox.poll(AgentId::new()).is_none());
    }
}
