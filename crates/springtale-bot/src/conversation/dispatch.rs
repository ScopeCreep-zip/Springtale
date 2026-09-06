//! Running a platform verb the NLU recognised (plan 5.4).
//!
//! The language understanding and the `/formation` command must not
//! drift apart, so a recognised verb is executed by the same builtin
//! handler the slash command hits — the sentence is turned into the
//! argument line and dispatched, nothing more.

use crate::conversation::catalog::IntentDoc;
use crate::conversation::dialogue::slots;
use crate::handler::registry::{HandlerContext, HandlerResult};
use crate::runtime::lifecycle::Bot;
use crate::state::session::SessionKey;

/// Build the argument line for a platform document from the utterance.
///
/// `formation.pause` + "hold the research squad" → `pause Research Squad`.
pub fn argument_line(doc: &IntentDoc, text: &str) -> Option<String> {
    let verb = doc.platform_verb?;
    let sub = verb.split_once('.').map(|(_, s)| s).unwrap_or(verb);
    let mut line = String::from(sub);
    for fill in slots::extract_all(text, doc) {
        if fill.slot_id == "formation" {
            line.push(' ');
            line.push_str(&fill.display);
        }
    }
    Some(line)
}

/// Execute a recognised platform verb through its builtin handler.
/// `None` when the document is not a platform verb, the handler is not
/// registered, or the sentence did not name what the verb needs.
pub async fn run(bot: &Bot, key: &SessionKey, doc: &IntentDoc, text: &str) -> Option<String> {
    let verb = doc.platform_verb?;
    let command = verb.split_once('.').map(|(c, _)| c).unwrap_or(verb);
    let args = argument_line(doc, text)?;
    // A verb that needs a formation but got none is not a match — fall
    // through so the user gets the ordinary "which one?" path rather
    // than a silently wrong target.
    if doc.slot("formation").is_some() && args.split_whitespace().count() < 2 {
        return None;
    }
    let handler = bot.handlers.get(command)?;
    let ctx = HandlerContext {
        user_id: key.user_id.clone(),
        channel_id: key.channel_id.clone(),
        source_connector: "chat".to_owned(),
        store: bot.store.clone(),
        registry: bot.registry.clone(),
        engine: bot.engine.clone(),
        capability_bridge: bot.capability_bridge.clone(),
        sentinel: bot.sentinel.clone(),
        formation_tier: None,
        runtime: bot.runtime.clone(),
    };
    match handler.handle(&args, &ctx).await {
        Ok(HandlerResult { response }) => Some(response),
        Err(e) => Some(e.to_string()),
    }
}
