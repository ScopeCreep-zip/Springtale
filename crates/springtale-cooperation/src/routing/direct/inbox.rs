use std::collections::VecDeque;
use std::sync::Mutex;

use dashmap::DashMap;

use crate::cadence::AgentId;
use crate::routing::types::TaskId;

/// Per-agent FIFO of task ids directly assigned to them. Small hot path; a
/// `Mutex<VecDeque>` is fine — contention is per-agent, not global.
#[derive(Debug, Default)]
pub struct DirectInbox {
    per_agent: DashMap<AgentId, Mutex<VecDeque<TaskId>>>,
}

impl DirectInbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, agent: AgentId, task_id: TaskId) {
        let inbox = self.per_agent.entry(agent).or_default();
        if let Ok(mut q) = inbox.lock() {
            q.push_back(task_id);
        }
    }

    pub fn poll(&self, agent: AgentId) -> Option<TaskId> {
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

    #[test]
    fn push_and_poll_fifo() {
        let inbox = DirectInbox::new();
        let agent = AgentId::new();
        let t1 = uuid::Uuid::new_v4();
        let t2 = uuid::Uuid::new_v4();
        inbox.push(agent, t1);
        inbox.push(agent, t2);
        assert_eq!(inbox.len(agent), 2);
        assert_eq!(inbox.poll(agent), Some(t1));
        assert_eq!(inbox.poll(agent), Some(t2));
        assert_eq!(inbox.poll(agent), None);
    }

    #[test]
    fn different_agents_have_separate_inboxes() {
        let inbox = DirectInbox::new();
        let a = AgentId::new();
        let b = AgentId::new();
        inbox.push(a, uuid::Uuid::new_v4());
        assert_eq!(inbox.len(a), 1);
        assert_eq!(inbox.len(b), 0);
    }

    #[test]
    fn poll_empty_returns_none() {
        let inbox = DirectInbox::new();
        assert!(inbox.poll(AgentId::new()).is_none());
    }
}
