//! Knowledge graph — petgraph projection of the shared mental model.
//!
//! Per COOPERATION_IMPLEMENTATION_PLAN.md §21: rusqlite + petgraph.
//! The graph enables relationship queries:
//! - "Which agents can handle capability X?" (BFS from capability node)
//! - "What patterns involve agent A?" (neighbors of agent node)
//! - "Are capabilities X and Y related?" (path exists check)
//!
//! Per Siege: accumulated map knowledge as a navigable graph.
//! Per MH: pattern recognition through relationship tracking.

use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;

use crate::cadence::AgentId;
use crate::capability::CapabilityDecl;
use crate::types::PatternId;

/// Node types in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Concept {
    /// An agent in the formation.
    Agent(AgentId),
    /// A capability (connector action, skill).
    Capability(CapabilityDecl),
    /// A cooperation pattern (trigger string from learning.rs).
    Pattern(PatternId),
    /// A domain concept (key from domain_knowledge).
    Domain(String),
}

/// Edge types between concepts.
#[derive(Debug, Clone)]
pub enum Relation {
    /// Agent has this capability. Weight = confidence (0.0-1.0).
    HasCapability { confidence: f32 },
    /// Agent participates in this pattern.
    ParticipatesIn,
    /// Capability requires another capability.
    Requires,
    /// Two concepts are aliases.
    AliasOf,
    /// Pattern involves this domain concept.
    InvolvesDomain,
}

/// Petgraph-backed knowledge graph.
///
/// Projected from SharedMentalModel data. Rebuilt when model changes
/// significantly (not every tick — only when domain_knowledge or
/// cooperation_patterns change).
pub struct KnowledgeGraph {
    graph: DiGraph<Concept, Relation>,
    node_index: HashMap<Concept, NodeIndex>,
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self {
            graph: DiGraph::new(),
            node_index: HashMap::new(),
        }
    }
}

impl KnowledgeGraph {
    /// Build the graph from current mental model state.
    pub fn rebuild(&mut self, model: &super::SharedMentalModel) {
        self.graph.clear();
        self.node_index.clear();

        // Add agent nodes + capability edges
        for (agent_id, capabilities) in &model.capability_awareness {
            let agent_node = self.get_or_create_node(Concept::Agent(*agent_id));
            for cap in capabilities {
                let cap_node = self.get_or_create_node(Concept::Capability(cap.clone()));
                self.graph.add_edge(
                    agent_node,
                    cap_node,
                    Relation::HasCapability { confidence: 1.0 },
                );
            }
        }

        // Add pattern nodes + participant edges
        for pattern in &model.cooperation_patterns {
            let pattern_node = self.get_or_create_node(Concept::Pattern(pattern.trigger.clone()));
            for agent_id in &pattern.participants {
                let agent_node = self.get_or_create_node(Concept::Agent(*agent_id));
                self.graph
                    .add_edge(agent_node, pattern_node, Relation::ParticipatesIn);
            }
        }

        // Add domain knowledge nodes + connect to patterns whose trigger
        // matches the domain key (the simplest heuristic for "this pattern
        // involves this domain concept").
        for key in model.domain_knowledge.keys() {
            let domain_node = self.get_or_create_node(Concept::Domain(key.clone()));
            for pattern in &model.cooperation_patterns {
                if pattern.trigger.0.contains(key.as_str()) {
                    let pattern_node =
                        self.get_or_create_node(Concept::Pattern(pattern.trigger.clone()));
                    self.graph
                        .add_edge(pattern_node, domain_node, Relation::InvolvesDomain);
                }
            }
        }
    }

    /// Get or create a node for a concept.
    fn get_or_create_node(&mut self, concept: Concept) -> NodeIndex {
        if let Some(&idx) = self.node_index.get(&concept) {
            idx
        } else {
            let idx = self.graph.add_node(concept.clone());
            self.node_index.insert(concept, idx);
            idx
        }
    }

    /// Find all agents that have a given capability.
    pub fn agents_with_capability(&self, capability: &str) -> Vec<AgentId> {
        let cap_concept = Concept::Capability(CapabilityDecl::new(capability));
        let Some(&cap_idx) = self.node_index.get(&cap_concept) else {
            return vec![];
        };

        // Walk edges TO this capability node — agents that have it
        self.graph
            .neighbors_directed(cap_idx, petgraph::Direction::Incoming)
            .filter_map(|n| match &self.graph[n] {
                Concept::Agent(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    /// Find all patterns an agent participates in.
    pub fn agent_patterns(&self, agent_id: AgentId) -> Vec<PatternId> {
        let agent_concept = Concept::Agent(agent_id);
        let Some(&agent_idx) = self.node_index.get(&agent_concept) else {
            return vec![];
        };

        self.graph
            .neighbors_directed(agent_idx, petgraph::Direction::Outgoing)
            .filter_map(|n| match &self.graph[n] {
                Concept::Pattern(trigger) => Some(trigger.clone()),
                _ => None,
            })
            .collect()
    }

    /// Find all patterns that involve a given domain concept.
    ///
    /// Per §21: "what patterns involve this domain concept?" — walks
    /// incoming `InvolvesDomain` edges from pattern nodes to the domain node.
    pub fn patterns_involving_domain(&self, domain_key: &str) -> Vec<PatternId> {
        let domain_concept = Concept::Domain(domain_key.to_owned());
        let Some(&domain_idx) = self.node_index.get(&domain_concept) else {
            return vec![];
        };

        self.graph
            .neighbors_directed(domain_idx, petgraph::Direction::Incoming)
            .filter_map(|n| match &self.graph[n] {
                Concept::Pattern(trigger) => Some(trigger.clone()),
                _ => None,
            })
            .collect()
    }

    /// Get total node count.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get total edge count.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::mental_model::{CooperationPattern, SharedMentalModel};
    use std::time::Instant;

    #[test]
    fn test_rebuild_from_model() {
        let mut model = SharedMentalModel::default();
        let a = AgentId::new();
        let b = AgentId::new();

        model
            .capability_awareness
            .insert(a, vec!["github".into(), "slack".into()]);
        model.capability_awareness.insert(b, vec!["slack".into()]);

        let mut graph = KnowledgeGraph::default();
        graph.rebuild(&model);

        // 2 agents + 2 capabilities (github, slack) = 4 nodes
        assert_eq!(graph.node_count(), 4);
        // a→github, a→slack, b→slack = 3 edges
        assert_eq!(graph.edge_count(), 3);
    }

    #[test]
    fn test_agents_with_capability() {
        let mut model = SharedMentalModel::default();
        let a = AgentId::new();
        let b = AgentId::new();

        model.capability_awareness.insert(a, vec!["slack".into()]);
        model
            .capability_awareness
            .insert(b, vec!["slack".into(), "github".into()]);

        let mut graph = KnowledgeGraph::default();
        graph.rebuild(&model);

        let slack_agents = graph.agents_with_capability("slack");
        assert_eq!(slack_agents.len(), 2);

        let github_agents = graph.agents_with_capability("github");
        assert_eq!(github_agents.len(), 1);

        let unknown = graph.agents_with_capability("unknown");
        assert!(unknown.is_empty());
    }

    #[test]
    fn test_agent_patterns() {
        let mut model = SharedMentalModel::default();
        let a = AgentId::new();
        let b = AgentId::new();

        model.cooperation_patterns.push(CooperationPattern {
            trigger: "github+slack".into(),
            participants: vec![a, b],
            success_count: 3,
            failure_count: 0,
            last_used: Instant::now(),
        });

        let mut graph = KnowledgeGraph::default();
        graph.rebuild(&model);

        let patterns = graph.agent_patterns(a);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0], *"github+slack");
    }
}
