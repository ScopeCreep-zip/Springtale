//! Render a dialogue [`Move`] into a conversational reply string.
//!
//! The dialogue layer decides WHAT to say (which `Move`, with which
//! data); this module decides HOW, picking a phrasing from the
//! `phrasebook` keyed on a per-frame sequence counter so replies vary.
//! Pure + deterministic given `(move, seq)` — fully testable.

use super::phrasebook;
use super::reflect;

/// What to ask for when prompting a single slot.
#[derive(Debug, Clone)]
pub struct SlotPrompt {
    pub label: String,
    pub hint: Option<String>,
    /// Option labels for `Select`/preset slots (empty for free text).
    pub options: Vec<String>,
    pub secret: bool,
}

/// One line of a confirmation summary.
#[derive(Debug, Clone)]
pub struct SummaryLine {
    pub label: String,
    /// Already display-ready (Select label, masked secret, etc.).
    pub value: String,
    /// `true` when this value came from a recipe default, not the user —
    /// surfaced so the user knows it's an assumption they can change.
    pub assumed: bool,
}

/// The set of things the bot can say during setup.
#[derive(Debug, Clone)]
pub enum Move {
    Capability {
        examples: Vec<String>,
    },
    Ack {
        recipe: String,
        then: Box<Move>,
    },
    Ask {
        slot: SlotPrompt,
    },
    Reask {
        slot: SlotPrompt,
        reason: String,
    },
    Clarify {
        recipe_names: Vec<String>,
    },
    Confirm {
        lines: Vec<SummaryLine>,
    },
    Reconfirm {
        lines: Vec<SummaryLine>,
    },
    AskChange,
    SecretHandoff {
        recipe: String,
        credentials: Vec<String>,
    },
    LibraryHandoff {
        recipe: String,
        fields: Vec<String>,
    },
    Deployed {
        summary: String,
    },
    Cancelled,
    DeployUnavailable,
}

/// Render a move to text. `seq` selects the phrasing variant.
pub fn render(mv: &Move, seq: u64) -> String {
    match mv {
        Move::Capability { examples } => {
            fill_examples(phrasebook::pick(phrasebook::CAPABILITY, seq), examples)
        }
        Move::Ack { recipe, then } => {
            let ack = phrasebook::pick(phrasebook::ACK, seq).replace("{recipe}", recipe);
            format!("{ack} {}", render(then, seq))
        }
        Move::Ask { slot } => render_ask(slot, seq),
        Move::Reask { slot, reason } => phrasebook::pick(phrasebook::REASK, seq)
            .replace("{slot}", &slot.label.to_lowercase())
            .replace("{reason}", reason),
        Move::Clarify { recipe_names } => phrasebook::pick(phrasebook::CLARIFY, seq)
            .replace("{options}", &reflect::or_list(recipe_names)),
        Move::Confirm { lines } => {
            phrasebook::pick(phrasebook::CONFIRM, seq).replace("{summary}", &render_summary(lines))
        }
        Move::Reconfirm { lines } => phrasebook::pick(phrasebook::RECONFIRM, seq)
            .replace("{summary}", &render_summary(lines)),
        Move::AskChange => phrasebook::pick(phrasebook::ASK_CHANGE, seq).to_owned(),
        Move::SecretHandoff {
            recipe,
            credentials,
        } => phrasebook::pick(phrasebook::SECRET_HANDOFF, seq)
            .replace("{recipe}", recipe)
            .replace("{credentials}", &reflect::and_list(credentials)),
        Move::LibraryHandoff { recipe, fields } => {
            phrasebook::pick(phrasebook::LIBRARY_HANDOFF, seq)
                .replace("{recipe}", recipe)
                .replace("{fields}", &reflect::and_list(fields))
        }
        Move::Deployed { summary } => {
            phrasebook::pick(phrasebook::DEPLOYED, seq).replace("{summary}", summary)
        }
        Move::Cancelled => phrasebook::pick(phrasebook::CANCELLED, seq).to_owned(),
        Move::DeployUnavailable => phrasebook::pick(phrasebook::DEPLOY_UNAVAILABLE, seq).to_owned(),
    }
}

fn render_ask(slot: &SlotPrompt, seq: u64) -> String {
    let slot_l = slot.label.to_lowercase();
    let base = if slot.secret {
        phrasebook::pick(phrasebook::ASK_SECRET, seq).replace("{slot}", &slot_l)
    } else if slot.options.is_empty() {
        phrasebook::pick(phrasebook::ASK_FREE, seq).replace("{slot}", &slot_l)
    } else {
        phrasebook::pick(phrasebook::ASK_CHOICE, seq)
            .replace("{slot}", &slot_l)
            .replace("{options}", &reflect::or_list(&slot.options))
    };
    match &slot.hint {
        Some(h) if !slot.secret => format!("{base} ({h})"),
        _ => base,
    }
}

fn render_summary(lines: &[SummaryLine]) -> String {
    let parts: Vec<String> = lines
        .iter()
        .map(|l| {
            let v = reflect::emphasize(&l.value);
            if l.assumed {
                format!(
                    "{} {v} (my default — say so to change)",
                    l.label.to_lowercase()
                )
            } else {
                format!("{} {v}", l.label.to_lowercase())
            }
        })
        .collect();
    format!("{}.", reflect::and_list(&parts))
}

fn fill_examples(template: &str, examples: &[String]) -> String {
    template.replace("{examples}", &reflect::or_list(examples))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn choice_slot() -> SlotPrompt {
        SlotPrompt {
            label: "City".into(),
            hint: None,
            options: vec!["Phoenix".into(), "Tucson".into()],
            secret: false,
        }
    }

    #[test]
    fn test_ask_choice_lists_options() {
        let out = render(
            &Move::Ask {
                slot: choice_slot(),
            },
            0,
        );
        assert!(out.contains("Phoenix"));
        assert!(out.contains("Tucson"));
    }

    #[test]
    fn test_secret_prompt_mentions_vault() {
        let slot = SlotPrompt {
            label: "Bot token".into(),
            hint: Some("from @BotFather".into()),
            options: vec![],
            secret: true,
        };
        let out = render(&Move::Ask { slot }, 0);
        assert!(out.to_lowercase().contains("vault"));
    }

    #[test]
    fn test_variation_changes_with_seq() {
        let a = render(&Move::Cancelled, 0);
        let b = render(&Move::Cancelled, 1);
        assert_ne!(a, b, "expected a different phrasing for a different seq");
    }

    #[test]
    fn test_confirm_marks_assumptions() {
        let lines = vec![
            SummaryLine {
                label: "City".into(),
                value: "Phoenix".into(),
                assumed: true,
            },
            SummaryLine {
                label: "Time".into(),
                value: "8:00 AM".into(),
                assumed: false,
            },
        ];
        let out = render(&Move::Confirm { lines }, 0);
        assert!(out.contains("default"));
        assert!(out.contains("Phoenix"));
        assert!(out.contains("8:00 AM"));
    }
}
