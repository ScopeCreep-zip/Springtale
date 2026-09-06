//! The one registry of platform verbs.
//!
//! Chat, the NLU catalogue, and (plan 2.3) the AI tool list all read
//! this list. Adding a verb here is the only way a new thing becomes
//! sayable in chat, which is what makes the drum-rule test below
//! meaningful.

use super::verb::{PlatformVerb, VerbGroup};

/// Every verb chat may use, in help order.
const VERBS: &[PlatformVerb] = &[
    // ── formation ────────────────────────────────────────────────────
    PlatformVerb {
        name: "formation.list",
        description: "List the formations and their status.",
        group: VerbGroup::Inspection,
        read_only: true,
        args: &[],
    },
    PlatformVerb {
        name: "formation.get",
        description: "Show one formation: intent, momentum, members.",
        group: VerbGroup::Inspection,
        read_only: true,
        args: &["formation"],
    },
    PlatformVerb {
        name: "formation.deploy",
        description: "Deploy a formation so its members start working.",
        group: VerbGroup::Intervention,
        read_only: false,
        args: &["formation"],
    },
    PlatformVerb {
        name: "formation.pause",
        description: "Pause a formation, holding its members where they are.",
        group: VerbGroup::Intervention,
        read_only: false,
        args: &["formation"],
    },
    PlatformVerb {
        name: "formation.resume",
        description: "Resume a paused formation.",
        group: VerbGroup::Intervention,
        read_only: false,
        args: &["formation"],
    },
    PlatformVerb {
        name: "formation.dissolve",
        description: "Dissolve a formation and release its members.",
        group: VerbGroup::Intervention,
        read_only: false,
        args: &["formation"],
    },
    PlatformVerb {
        name: "formation.rally",
        description: "Send a rally to a formation, redirecting its attention.",
        group: VerbGroup::Intervention,
        read_only: false,
        args: &["formation"],
    },
    PlatformVerb {
        name: "formation.intent",
        description: "Cycle or set a formation's intent.",
        group: VerbGroup::Intent,
        read_only: false,
        args: &["formation", "intent"],
    },
    PlatformVerb {
        name: "formation.guard",
        description: "Toggle a formation's guard.",
        group: VerbGroup::Constraints,
        read_only: false,
        args: &["formation"],
    },
    PlatformVerb {
        name: "formation.add_member",
        description: "Add a connector to a formation as a member.",
        group: VerbGroup::Composition,
        read_only: false,
        args: &["formation", "connector"],
    },
    PlatformVerb {
        name: "formation.remove_member",
        description: "Remove a member connector from a formation.",
        group: VerbGroup::Composition,
        read_only: false,
        args: &["formation", "connector"],
    },
    // ── approvals ────────────────────────────────────────────────────
    PlatformVerb {
        name: "approvals.list",
        description: "List the approval requests still waiting.",
        group: VerbGroup::Inspection,
        read_only: true,
        args: &[],
    },
    PlatformVerb {
        name: "approvals.approve",
        description: "Approve a pending request by id.",
        group: VerbGroup::Intervention,
        read_only: false,
        args: &["id"],
    },
    PlatformVerb {
        name: "approvals.deny",
        description: "Deny a pending request by id.",
        group: VerbGroup::Intervention,
        read_only: false,
        args: &["id"],
    },
    // ── memory ───────────────────────────────────────────────────────
    PlatformVerb {
        name: "memory.audit",
        description: "Audit what the bot remembers and how it is encrypted.",
        group: VerbGroup::Inspection,
        read_only: true,
        args: &[],
    },
    PlatformVerb {
        name: "memory.compact",
        description: "Compact stored memory, dropping rows past the retention window.",
        group: VerbGroup::Intervention,
        read_only: false,
        args: &[],
    },
    // ── safety ───────────────────────────────────────────────────────
    PlatformVerb {
        name: "safety.get",
        description: "Show the safety configuration.",
        group: VerbGroup::Inspection,
        read_only: true,
        args: &[],
    },
    PlatformVerb {
        name: "safety.set",
        description: "Change a safety setting (requires --confirm).",
        group: VerbGroup::Constraints,
        read_only: false,
        args: &["key", "value"],
    },
    // ── model configuration ──────────────────────────────────────────
    PlatformVerb {
        name: "ai.get",
        description: "Show the configured AI adapter and model.",
        group: VerbGroup::Inspection,
        read_only: true,
        args: &[],
    },
    PlatformVerb {
        name: "ai.set",
        description: "Set the AI adapter (none, ollama, openai, anthropic).",
        group: VerbGroup::Constraints,
        read_only: false,
        args: &["adapter"],
    },
];

/// The canonical platform verb list.
pub fn platform_verbs() -> &'static [PlatformVerb] {
    VERBS
}

/// Look one verb up by dotted name.
pub fn find_verb(name: &str) -> Option<&'static PlatformVerb> {
    VERBS.iter().find(|v| v.name == name)
}

/// The distinct chat command names (`formation`, `approvals`, …).
pub fn verb_commands() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for v in VERBS {
        if !out.contains(&v.command()) {
            out.push(v.command());
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The drum rule: you steer a formation, you never hand work to a
    /// named member. The CLI asserts this over its clap tree
    /// (`apps/springtale-cli/src/cli.rs`); chat asserts it here, over
    /// the registry every chat surface reads.
    #[test]
    fn test_no_chat_verb_assigns_work_to_a_named_member() {
        let offenders: Vec<&str> = platform_verbs()
            .iter()
            .filter(|v| {
                v.name.contains("assign")
                    || v.description.to_lowercase().contains("assign")
                    || v.args.contains(&"member")
                    || v.args.contains(&"agent")
            })
            .map(|v| v.name)
            .collect();
        assert!(
            offenders.is_empty(),
            "assign verb(s) present, drum rule violated: {offenders:?}"
        );
    }

    /// Every chat verb belongs to one of the four orchestration groups
    /// or to read-only inspection — and nothing else.
    #[test]
    fn test_chat_verbs_stay_in_the_four_groups_plus_inspection() {
        for v in platform_verbs() {
            assert!(
                matches!(
                    v.group,
                    VerbGroup::Inspection
                        | VerbGroup::Composition
                        | VerbGroup::Intent
                        | VerbGroup::Constraints
                        | VerbGroup::Intervention
                ),
                "verb `{}` is outside the four groups",
                v.name
            );
            // Inspection is exactly the read-only set.
            assert_eq!(
                v.read_only,
                v.group == VerbGroup::Inspection,
                "verb `{}` disagrees with its group about being read-only",
                v.name
            );
        }
    }

    #[test]
    fn test_verb_names_are_unique_and_dotted() {
        let mut seen = std::collections::HashSet::new();
        for v in platform_verbs() {
            assert!(v.name.contains('.'), "verb `{}` is not dotted", v.name);
            assert!(seen.insert(v.name), "duplicate verb `{}`", v.name);
        }
    }

    #[test]
    fn test_input_schema_lists_every_argument() {
        let verb = find_verb("formation.add_member").expect("verb exists");
        let schema = verb.input_schema();
        let props = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("object schema");
        assert!(props.contains_key("formation"));
        assert!(props.contains_key("connector"));
    }
}
