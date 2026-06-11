//! Optional AI augmentation — strictly additive over the deterministic
//! engine, and removable without breaking anything.
//!
//! The base path (`engine`) never consults an LLM. This module adds ONE
//! enhancement: when the deterministic intent matcher finds no recipe,
//! an available adapter may map the fuzzy request onto a catalogue
//! recipe id. Crucially, the AI only chooses WHICH recipe — the slot
//! filling, validation, confirmation, and deploy that follow are 100%
//! deterministic ([`engine::start_recipe`]). With a `NoopAdapter`
//! (`is_available()` == false) this whole module short-circuits to
//! `Ok(None)` and the bot behaves exactly as if it weren't here.

use std::time::Duration;

use crate::runtime::lifecycle::Bot;
use crate::state::session::SessionKey;

use super::engine;
use super::error::ConversationError;

/// If an AI adapter is available, ask it to map `text` to a recipe and
/// start a deterministic setup frame for it. `Ok(None)` when AI is
/// absent or declines — the caller then falls through to the existing
/// free-form `ai_fallback` and finally the deterministic capability
/// reply.
pub async fn ai_assisted_start(
    bot: &Bot,
    key: &SessionKey,
    text: &str,
) -> Result<Option<String>, ConversationError> {
    if !bot.ai_adapter.is_available().await {
        return Ok(None);
    }

    let catalog = engine::build_catalog(bot).await?;
    if catalog.intents.is_empty() {
        return Ok(None);
    }

    let Some(recipe_id) = match_recipe_via_ai(bot, text, &catalog).await else {
        return Ok(None);
    };

    // The recipe choice came from AI; everything after is deterministic.
    engine::start_recipe(bot, key, &recipe_id, text).await
}

/// Build a constrained prompt and ask the adapter to pick a recipe id.
/// Best-effort: any error / unparseable answer yields `None`.
async fn match_recipe_via_ai(
    bot: &Bot,
    text: &str,
    catalog: &super::catalog::CatalogSnapshot,
) -> Option<String> {
    let mut menu = String::new();
    for d in &catalog.intents {
        menu.push_str(&format!(
            "- {}: {} — {}\n",
            d.recipe_id, d.name, d.description
        ));
    }

    let prompt = format!(
        "You map a user's request to exactly one automation recipe id from this list, \
         or reply NONE if nothing fits. Reply with ONLY the id (or NONE), no other text.\n\n\
         Recipes:\n{menu}\nUser request: \"{text}\"\nRecipe id:"
    );

    let options = springtale_ai::AiOptions {
        max_tokens: 24,
        timeout: Duration::from_secs(10),
        temperature: Some(0.0),
    };

    let response = bot
        .ai_adapter
        .complete(springtale_ai::AiRequest::Complete { prompt }, options)
        .await
        .ok()?;

    let answer = response.content.trim().trim_matches('"').to_lowercase();
    if answer.is_empty() || answer.contains("none") {
        return None;
    }

    // Only accept an id that actually exists in the catalogue — the AI
    // can never invent a recipe, only select one.
    catalog
        .intents
        .iter()
        .find(|d| d.recipe_id.to_lowercase() == answer)
        .map(|d| d.recipe_id.clone())
}
