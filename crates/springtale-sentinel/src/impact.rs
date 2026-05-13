use springtale_core::rule::action::Action;

/// Classification of an action's impact level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionImpact {
    /// Action only reads data, no side effects.
    ReadOnly,
    /// Action has side effects but can be undone or is low-risk.
    Reversible,
    /// Action has irreversible or high-impact side effects.
    Destructive,
}

/// Classify the impact level of an action.
pub fn classify_impact(action: &Action) -> ActionImpact {
    match action {
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

        // Destructive: file writes (especially with delete), shell execution
        Action::WriteFile {
            delete_source: true,
            ..
        } => ActionImpact::Destructive,
        Action::RunShell { .. } => ActionImpact::Destructive,

        // Reversible: most connector actions, messages, notifications, file writes without delete
        Action::RunConnector { .. }
        | Action::SendMessage { .. }
        | Action::Notify { .. }
        | Action::WriteFile { .. } => ActionImpact::Reversible,

        // Chain: classified by the most impactful step
        Action::Chain { steps } => steps
            .iter()
            .map(classify_impact)
            .max_by_key(|impact| match impact {
                ActionImpact::ReadOnly => 0,
                ActionImpact::Reversible => 1,
                ActionImpact::Destructive => 2,
            })
            .unwrap_or(ActionImpact::ReadOnly),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_is_read_only() {
        assert_eq!(
            classify_impact(&Action::Delay { seconds: 5 }),
            ActionImpact::ReadOnly
        );
    }

    #[test]
    fn test_send_message_is_reversible() {
        assert_eq!(
            classify_impact(&Action::SendMessage { text: "hi".into() }),
            ActionImpact::Reversible
        );
    }

    #[test]
    fn test_run_shell_is_destructive() {
        assert_eq!(
            classify_impact(&Action::RunShell {
                command: "rm -rf".into()
            }),
            ActionImpact::Destructive
        );
    }

    #[test]
    fn test_write_file_with_delete_is_destructive() {
        assert_eq!(
            classify_impact(&Action::WriteFile {
                destination: "/tmp/x".into(),
                content: "data".into(),
                delete_source: true,
            }),
            ActionImpact::Destructive
        );
    }

    #[test]
    fn test_write_file_without_delete_is_reversible() {
        assert_eq!(
            classify_impact(&Action::WriteFile {
                destination: "/tmp/x".into(),
                content: "data".into(),
                delete_source: false,
            }),
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
        assert_eq!(classify_impact(&chain), ActionImpact::Destructive);
    }

    #[test]
    fn test_empty_chain_is_read_only() {
        assert_eq!(
            classify_impact(&Action::Chain { steps: vec![] }),
            ActionImpact::ReadOnly
        );
    }
}
