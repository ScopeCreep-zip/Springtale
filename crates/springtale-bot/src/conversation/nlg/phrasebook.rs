//! Static template banks for conversational replies.
//!
//! Each `Move` has several phrasings; the renderer rotates through them
//! (keyed on a per-frame sequence counter) so the bot doesn't repeat
//! itself turn after turn — the cheap, deterministic way to dodge the
//! "stiff style" that makes rule-based bots feel robotic. Placeholders:
//! `{recipe}`, `{slot}`, `{options}`, `{value}`, `{summary}`,
//! `{examples}`.

/// Acknowledge a recognized request before asking the first question.
pub const ACK: &[&str] = &[
    "Love it — let's set up {recipe}.",
    "On it: {recipe}.",
    "Sure — {recipe} coming up.",
    "Happy to. Setting up {recipe}.",
];

/// Ask for a slot that has a fixed set of choices.
pub const ASK_CHOICE: &[&str] = &[
    "Which {slot}? You can pick {options}.",
    "What {slot} should I use — {options}?",
    "Pick a {slot}: {options}.",
];

/// Ask for a free-form slot.
pub const ASK_FREE: &[&str] = &[
    "What {slot} should I use?",
    "Tell me the {slot}.",
    "What's the {slot}?",
];

/// Ask for a secret.
pub const ASK_SECRET: &[&str] = &[
    "Paste your {slot} — I'll store it in the vault and never show it in chat.",
    "Send me the {slot}; it goes straight into the encrypted vault, never plain text.",
];

/// Re-ask after an answer that didn't validate.
pub const REASK: &[&str] = &[
    "Hmm, that didn't look like a valid {slot}. {reason} Mind trying again?",
    "I couldn't use that as the {slot}. {reason}",
];

/// Ask the user to choose between several matching recipes.
pub const CLARIFY: &[&str] = &[
    "A few things fit that — did you mean {options}?",
    "I can do that a couple of ways: {options}. Which one?",
];

/// Summarize and ask for the go-ahead.
pub const CONFIRM: &[&str] = &[
    "Here's the plan: {summary} Want me to set it up?",
    "Ready when you are — {summary} Shall I deploy it?",
    "Got it all: {summary} Good to go?",
];

/// Re-confirm after a mid-flow correction.
pub const RECONFIRM: &[&str] = &[
    "Updated — {summary} Good to deploy now?",
    "Done: {summary} Ready?",
];

/// Success.
pub const DEPLOYED: &[&str] = &[
    "All set — {summary} 🎉",
    "Done! {summary}",
    "You're live: {summary}",
];

/// User backed out.
pub const CANCELLED: &[&str] = &[
    "No problem, I've dropped that. Ask anytime.",
    "Okay, cancelled. Just say the word when you want to try again.",
];

/// A recipe needs credentials — hand off to the secure setup flow
/// rather than collecting a secret in chat.
pub const SECRET_HANDOFF: &[&str] = &[
    "I can set up {recipe}, but it needs your {credentials}. For your safety I don't take credentials in chat — open {recipe} in the Library and I'll keep them in the vault.",
    "{recipe} needs {credentials}. I keep secrets out of chat — add them securely from the Library and it's good to go.",
];

/// A recipe needs an input that's easier to set in the Library (a
/// selector picker, a destination chooser) than to type in chat.
pub const LIBRARY_HANDOFF: &[&str] = &[
    "I can set up {recipe}, but it needs {fields}, which is easier to pick in the Library — open it there and you're set.",
    "{recipe} needs {fields} — that's a tap in the Library rather than something to type here. Open it there to finish.",
];

/// Confirm step: the user wants to change something but didn't say what.
pub const ASK_CHANGE: &[&str] = &[
    "Sure — what would you like to change?",
    "No problem. What should I change — just tell me the new value.",
];

/// The "what can you do?" answer.
pub const CAPABILITY: &[&str] = &[
    "I can wire up little automations for you — like {examples}. Just tell me what you want in plain words.",
    "Tell me what you'd like and I'll set it up — for example {examples}.",
];

/// Deploy backend not wired (headless/test contexts).
pub const DEPLOY_UNAVAILABLE: &[&str] =
    &["I've got everything I need, but I can't deploy from here right now."];

/// Pick one phrasing from a bank, rotating on `seq`.
pub fn pick(bank: &[&'static str], seq: u64) -> &'static str {
    if bank.is_empty() {
        return "";
    }
    bank[(seq as usize) % bank.len()]
}
