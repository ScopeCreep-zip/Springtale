use springtale_core::rule::action::Action;

/// Classification of an action's impact level.
///
/// Ordered `ReadOnly < Reversible < Destructive` so a chain's impact is
/// the `max` of its steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionImpact {
    /// Action only reads data, no side effects.
    ReadOnly,
    /// Action has side effects but can be undone or is low-risk.
    Reversible,
    /// Action has irreversible or high-impact side effects.
    Destructive,
}

/// The manifest's advisory hints for one connector action.
///
/// Mirrors MCP `ToolAnnotations` (`readOnlyHint` / `destructiveHint`).
/// Advisory only — the sentinel decides. A hint never widens what an
/// action may do; it only lets a declared-safe action skip the gate.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionHints {
    /// The action only retrieves data (MCP `readOnlyHint`).
    pub read_only: bool,
    /// MCP `destructiveHint`. `None` is unknown and counts as `true`.
    pub destructive: Option<bool>,
}

/// Classify the impact level of an action.
///
/// `hints` are the manifest's declarations for the named connector action
/// when `action` is a [`Action::RunConnector`]. `None` — the connector or
/// action is unknown — classifies as destructive, matching MCP's
/// `destructiveHint` default of `true`. Hints are ignored for every other
/// action kind; a [`Action::Chain`] passes them to each step.
pub fn classify_impact(action: &Action, hints: Option<ActionHints>) -> ActionImpact {
    match action {
        // Connector actions: trust the manifest's hints only in the safe
        // direction. Unknown action, unknown hint, or `destructive: true`
        // all classify as destructive.
        Action::RunConnector { .. } => match hints {
            Some(h) if h.read_only => ActionImpact::ReadOnly,
            Some(ActionHints {
                destructive: Some(false),
                ..
            }) => ActionImpact::Reversible,
            _ => ActionImpact::Destructive,
        },

        // Destructive: file writes with delete, shell execution.
        Action::WriteFile {
            delete_source: true,
            ..
        }
        | Action::RunShell { .. } => ActionImpact::Destructive,

        // Read-only: no external side effects.
        // - Transform: pure data shaping.
        // - Delay: just sleeps.
        // - AiComplete: posts a prompt to the configured adapter and
        //   reads back; for impact-classification purposes treated as
        //   read-only (the adapter call itself is sandboxed).
        // - Extract (Phase A): parses bytes from chain state, no I/O.
        // - Dedupe (Phase A): touches only the local `dedupe_seen`
        //   table — a privacy-safe blake3 hash + LRU prune. No
        //   external side effects.
        Action::Transform { .. }
        | Action::Delay { .. }
        | Action::AiComplete { .. }
        | Action::Extract { .. }
        | Action::Dedupe { .. } => ActionImpact::ReadOnly,

        // Reversible: messages, notifications, file writes without delete.
        Action::SendMessage { .. } | Action::Notify { .. } | Action::WriteFile { .. } => {
            ActionImpact::Reversible
        }

        // Chain: classified by the most impactful step.
        Action::Chain { steps } => steps
            .iter()
            .map(|s| classify_impact(s, hints))
            .max()
            .unwrap_or(ActionImpact::ReadOnly),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn run_connector() -> Action {
        Action::RunConnector {
            connector: "connector-test".into(),
            action: "do_thing".into(),
            params: serde_json::Map::new(),
        }
    }

    #[test]
    fn test_delay_is_read_only() {
        assert_eq!(
            classify_impact(&Action::Delay { seconds: 5 }, None),
            ActionImpact::ReadOnly
        );
    }

    #[test]
    fn test_send_message_is_reversible() {
        assert_eq!(
            classify_impact(&Action::SendMessage { text: "hi".into() }, None),
            ActionImpact::Reversible
        );
    }

    #[test]
    fn test_run_shell_is_destructive() {
        assert_eq!(
            classify_impact(
                &Action::RunShell {
                    command: "rm -rf".into()
                },
                None
            ),
            ActionImpact::Destructive
        );
    }

    #[test]
    fn test_write_file_with_delete_is_destructive() {
        assert_eq!(
            classify_impact(
                &Action::WriteFile {
                    destination: "/tmp/x".into(),
                    content: "data".into(),
                    delete_source: true,
                },
                None
            ),
            ActionImpact::Destructive
        );
    }

    #[test]
    fn test_write_file_without_delete_is_reversible() {
        assert_eq!(
            classify_impact(
                &Action::WriteFile {
                    destination: "/tmp/x".into(),
                    content: "data".into(),
                    delete_source: false,
                },
                None
            ),
            ActionImpact::Reversible
        );
    }

    #[test]
    fn test_chain_inherits_worst_impact() {
        let chain = Action::Chain {
            steps: vec![
                Action::Delay { seconds: 1 },
                Action::SendMessage { text: "hi".into() },
                Action::RunShell {
                    command: "ls".into(),
                },
            ],
        };
        assert_eq!(classify_impact(&chain, None), ActionImpact::Destructive);
    }

    #[test]
    fn test_empty_chain_is_read_only() {
        assert_eq!(
            classify_impact(&Action::Chain { steps: vec![] }, None),
            ActionImpact::ReadOnly
        );
    }

    #[test]
    fn test_run_connector_without_hints_is_destructive() {
        assert_eq!(
            classify_impact(&run_connector(), None),
            ActionImpact::Destructive
        );
    }

    #[test]
    fn test_run_connector_read_only_hint_is_read_only() {
        let hints = ActionHints {
            read_only: true,
            destructive: None,
        };
        assert_eq!(
            classify_impact(&run_connector(), Some(hints)),
            ActionImpact::ReadOnly
        );
    }

    #[test]
    fn test_run_connector_destructive_false_hint_is_reversible() {
        let hints = ActionHints {
            read_only: false,
            destructive: Some(false),
        };
        assert_eq!(
            classify_impact(&run_connector(), Some(hints)),
            ActionImpact::Reversible
        );
    }

    #[test]
    fn test_chain_of_read_only_and_reversible_is_reversible() {
        let chain = Action::Chain {
            steps: vec![
                Action::Delay { seconds: 1 },
                Action::SendMessage { text: "hi".into() },
            ],
        };
        assert_eq!(classify_impact(&chain, None), ActionImpact::Reversible);
    }
}
