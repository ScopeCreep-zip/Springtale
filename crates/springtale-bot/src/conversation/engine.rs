//! The conversational engine — orchestrates NLU → dialogue → NLG → deploy.
//!
//! Two entry points the message handler calls:
//!   - [`continue_active`] runs BEFORE the command router: if there's a
//!     live setup frame, the message is a dialogue turn (a slot answer,
//!     a correction, a yes/no), never a command.
//!   - [`try_start`] runs in the router's `NoMatch` arm: attempt to
//!     recognize a NEW setup request. Returns `None` (unhandled) so the
//!     caller can fall through to AI / the capability reply.
//!
//! Everything here is deterministic — no AI is consulted. The optional
//! AI augmentation lives in [`super::augment`] and only ever runs after
//! this engine has declined.

use crate::runtime::lifecycle::Bot;
use crate::state::session::{Session, SessionKey, load_or_create_session, save_session};

use super::catalog::{CatalogSnapshot, IntentDoc};
use super::dialogue::frame::{Frame, FrameStep};
use super::dialogue::transition::{self, TurnAction};
use super::error::ConversationError;
use super::nlg::{self, Move};
use super::nlu::intent::{self, IntentDecision};

use springtale_runtime::operations::recipes::types::RecipeFilter;

/// Run a dialogue turn if a setup frame is active. `Ok(None)` means
/// "no active frame — not my turn", so the caller proceeds to routing.
pub async fn continue_active(
    bot: &Bot,
    key: &SessionKey,
    text: &str,
) -> Result<Option<String>, ConversationError> {
    let mut session = load_or_create_session(&bot.store, key).await?;
    let now = chrono::Utc::now();
    let Some(mut frame) = Frame::load(&session, now) else {
        return Ok(None);
    };
    frame.bump_seq();

    let catalog = build_catalog(bot).await?;
    let reply = drive(bot, &mut session, &mut frame, &catalog, text).await?;
    save_session(&bot.store, &session).await?;
    Ok(Some(reply))
}

/// Attempt to recognize a NEW setup request. `Ok(None)` = no recipe
/// matched (the caller falls through to AI / capability reply).
pub async fn try_start(
    bot: &Bot,
    key: &SessionKey,
    text: &str,
) -> Result<Option<String>, ConversationError> {
    let catalog = build_catalog(bot).await?;
    let decision = intent::decide(intent::rank(text, &catalog));
    let now = chrono::Utc::now();

    let mut session = load_or_create_session(&bot.store, key).await?;
    let reply = match decision {
        IntentDecision::Confident(cand) => {
            let Some(doc) = catalog.find(&cand.recipe_id).cloned() else {
                return Ok(None);
            };
            // A platform verb is run, not set up: no slot-filling frame,
            // no deploy. Same handler the slash command reaches.
            if doc.platform_verb.is_some() {
                match super::dispatch::run(bot, key, &doc, text).await {
                    Some(reply) => return Ok(Some(reply)),
                    None => return Ok(None),
                }
            }
            start_frame(&mut session, &doc, text, now)
        }
        IntentDecision::Ambiguous(cands) => {
            let names = cands.iter().map(|c| c.name.clone()).collect();
            let ids = cands.iter().map(|c| c.recipe_id.clone()).collect();
            let frame = Frame::clarifying(ids, text, now);
            let reply = nlg::render(
                &Move::Clarify {
                    recipe_names: names,
                },
                frame.seq,
            );
            frame.store_into(&mut session);
            reply
        }
        IntentDecision::NoMatch => return Ok(None),
    };
    save_session(&bot.store, &session).await?;
    Ok(Some(reply))
}

/// Start a deterministic setup frame for a KNOWN recipe id. Used by the
/// optional AI-assist seam ([`super::augment`]): the AI only maps a
/// fuzzy request to a recipe; the slot-filling that follows is 100%
/// deterministic. `Ok(None)` when the id isn't in the catalogue.
pub async fn start_recipe(
    bot: &Bot,
    key: &SessionKey,
    recipe_id: &str,
    utterance: &str,
) -> Result<Option<String>, ConversationError> {
    let catalog = build_catalog(bot).await?;
    let Some(doc) = catalog.find(recipe_id).cloned() else {
        return Ok(None);
    };
    let mut session = load_or_create_session(&bot.store, key).await?;
    let reply = start_frame(&mut session, &doc, utterance, chrono::Utc::now());
    save_session(&bot.store, &session).await?;
    Ok(Some(reply))
}

/// The deterministic "what can you do?" answer — used as the final
/// fallback (replacing the old static suggestion) when no command, no
/// frame, and no AI handle the message.
pub async fn capability_reply(bot: &Bot) -> Result<String, ConversationError> {
    let catalog = build_catalog(bot).await?;
    let examples: Vec<String> = catalog
        .intents
        .iter()
        .filter(|d| !d.ai_required)
        .take(3)
        .map(|d| d.name.clone())
        .collect();
    Ok(nlg::render(&Move::Capability { examples }, 0))
}

// ── internals ────────────────────────────────────────────────────────

pub(super) async fn build_catalog(bot: &Bot) -> Result<CatalogSnapshot, ConversationError> {
    let recipes =
        springtale_runtime::operations::recipes::list_recipes(&*bot.store, RecipeFilter::default())
            .await?;
    // Plan 5.4 — the platform verbs are documents too, and their
    // `{formation}` slot list is the live roster read here, at match
    // time, not a hard-coded list.
    let formation_names = live_formation_names(bot).await;
    Ok(CatalogSnapshot::build_with_platform(
        recipes,
        CHAT_LOCALE,
        &formation_names,
    ))
}

/// The locale the chat sentence templates are read in. Only `en` is
/// translated today; the other seven files are stubs that fall back to
/// it (`conversation::sentences`).
const CHAT_LOCALE: &str = "en";

/// Formation names from the store, or none when this bot has no runtime
/// (headless / CLI / tests) — then the platform documents simply carry
/// an empty slot list and never win a match.
async fn live_formation_names(bot: &Bot) -> Vec<String> {
    let Some(rt) = bot.runtime.as_ref() else {
        return Vec::new();
    };
    match springtale_runtime::operations::formations::list_formations(rt).await {
        Ok(list) => list.into_iter().map(|f| f.name).collect(),
        Err(e) => {
            tracing::warn!(error = %e, "formation roster unavailable for chat slots");
            Vec::new()
        }
    }
}

/// Choose the right hand-off message: a security-framed one for secrets
/// (vault), or a "set it up in the Library" one for picker/editor inputs.
fn handoff_move(doc: &IntentDoc) -> Move {
    if doc.requires_secret() {
        Move::SecretHandoff {
            recipe: doc.name.clone(),
            credentials: doc.required_secret_labels(),
        }
    } else {
        Move::LibraryHandoff {
            recipe: doc.name.clone(),
            fields: doc.handoff_labels(),
        }
    }
}

/// Build a fresh collecting frame, seed it from the opening utterance,
/// and render the ack + first prompt/confirmation.
fn start_frame(
    session: &mut Session,
    doc: &IntentDoc,
    text: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    // Some inputs can't be collected in chat — credentials (would land in
    // the session store as plaintext) and picker/editor kinds. Hand those
    // recipes off to the Library instead of dead-ending in a re-ask loop.
    if doc.requires_handoff() {
        return nlg::render(&handoff_move(doc), 0);
    }
    let mut frame = Frame::collecting(&doc.recipe_id, text, now);
    let inner = transition::seed_and_advance(&mut frame, doc, text);
    let mv = Move::Ack {
        recipe: doc.name.clone(),
        then: Box::new(inner),
    };
    let reply = nlg::render(&mv, frame.seq);
    frame.store_into(session);
    reply
}

/// Advance an active frame by one user message.
async fn drive(
    bot: &Bot,
    session: &mut Session,
    frame: &mut Frame,
    catalog: &CatalogSnapshot,
    text: &str,
) -> Result<String, ConversationError> {
    // Clarifying is resolved here (needs the catalogue to pick the recipe).
    if frame.step == FrameStep::Clarifying {
        return Ok(resolve_clarify(session, frame, catalog, text));
    }

    let Some(doc) = frame
        .recipe_id
        .as_deref()
        .and_then(|id| catalog.find(id))
        .cloned()
    else {
        // The recipe disappeared from the catalogue — drop the frame.
        Frame::clear(session);
        return Ok("That recipe isn't available anymore — let's start over.".to_owned());
    };

    match transition::step(frame, &doc, text) {
        TurnAction::Speak(mv) => {
            let reply = nlg::render(&mv, frame.seq);
            frame.store_into(session);
            Ok(reply)
        }
        TurnAction::Cancel(mv) => {
            let reply = nlg::render(&mv, frame.seq);
            Frame::clear(session);
            Ok(reply)
        }
        TurnAction::Deploy => {
            let outcome = deploy_now(bot, frame, &doc).await;
            match outcome {
                DeployOutcome::Done(reply) => {
                    Frame::clear(session);
                    Ok(reply)
                }
                DeployOutcome::Blocked(reply) => {
                    // Keep the frame open so the user can fix the blocker.
                    frame.store_into(session);
                    Ok(reply)
                }
            }
        }
    }
}

/// Resolve a clarification: pick the chosen recipe (by number or name),
/// then seed a collecting frame from the original utterance.
fn resolve_clarify(
    session: &mut Session,
    frame: &mut Frame,
    catalog: &CatalogSnapshot,
    text: &str,
) -> String {
    if super::nlu::entities::is_cancel(text) {
        Frame::clear(session);
        return nlg::render(&Move::Cancelled, frame.seq);
    }

    let chosen = pick_candidate(&frame.candidates, catalog, text);
    let Some(recipe_id) = chosen else {
        // Couldn't tell — re-ask with the same options.
        let names = frame
            .candidates
            .iter()
            .filter_map(|id| catalog.find(id).map(|d| d.name.clone()))
            .collect();
        frame.store_into(session);
        return nlg::render(
            &Move::Clarify {
                recipe_names: names,
            },
            frame.seq,
        );
    };

    let Some(doc) = catalog.find(&recipe_id).cloned() else {
        Frame::clear(session);
        return "That option isn't available anymore — let's start over.".to_owned();
    };

    // Recipes needing an uncollectable input are handed off, never
    // collected in chat.
    if doc.requires_handoff() {
        Frame::clear(session);
        return nlg::render(&handoff_move(&doc), frame.seq);
    }

    // Re-anchor the frame onto the chosen recipe and seed from the
    // original sentence so a one-shot request still pre-fills.
    let now = chrono::Utc::now();
    let mut fresh = Frame::collecting(&recipe_id, &frame.original_utterance, now);
    fresh.seq = frame.seq;
    let inner = transition::seed_and_advance(&mut fresh, &doc, &frame.original_utterance);
    let mv = Move::Ack {
        recipe: doc.name.clone(),
        then: Box::new(inner),
    };
    let reply = nlg::render(&mv, fresh.seq);
    fresh.store_into(session);
    reply
}

/// Match the user's disambiguation reply to one candidate: a 1-based
/// number, or a name/keyword overlap.
fn pick_candidate(candidates: &[String], catalog: &CatalogSnapshot, text: &str) -> Option<String> {
    let trimmed = text.trim();
    if let Ok(n) = trimmed.parse::<usize>()
        && n >= 1
        && n <= candidates.len()
    {
        return candidates.get(n - 1).cloned();
    }

    // Score each candidate by name-token overlap with the reply.
    let reply_stems: Vec<String> = super::nlu::tokenize(text)
        .into_iter()
        .map(|t| t.stem)
        .collect();
    let mut best: Option<(usize, &String)> = None;
    for id in candidates {
        let Some(doc) = catalog.find(id) else {
            continue;
        };
        let hits = doc
            .name_stems
            .iter()
            .filter(|s| reply_stems.contains(s))
            .count();
        if hits > 0 && best.is_none_or(|(b, _)| hits > b) {
            best = Some((hits, id));
        }
    }
    best.map(|(_, id)| id.clone())
}

enum DeployOutcome {
    Done(String),
    Blocked(String),
}

async fn deploy_now(bot: &Bot, frame: &Frame, doc: &IntentDoc) -> DeployOutcome {
    let inputs = frame.to_recipe_inputs();
    let recipe_id = doc.recipe_id.clone();

    let Some(deployer) = bot.recipe_deployer.clone() else {
        return DeployOutcome::Done(nlg::render(&Move::DeployUnavailable, frame.seq));
    };

    // Preflight first — surface blocking items conversationally instead
    // of a silent "deployed but does nothing".
    if let Ok(report) = deployer.preflight(&recipe_id, &inputs).await
        && !report.deployable
    {
        let blockers: Vec<String> = report
            .items
            .iter()
            .filter(|i| {
                matches!(
                    i.status,
                    springtale_runtime::operations::preflight::types::PreflightStatus::Blocking
                )
            })
            .filter_map(|i| i.detail.clone())
            .collect();
        if !blockers.is_empty() {
            return DeployOutcome::Blocked(format!(
                "Almost there — {} Fix that and say 'go'.",
                blockers.join(" ")
            ));
        }
    }

    match deployer.deploy(&recipe_id, inputs).await {
        Ok(report) => DeployOutcome::Done(nlg::render(
            &Move::Deployed {
                summary: report.summary,
            },
            frame.seq,
        )),
        Err(e) => DeployOutcome::Blocked(format!(
            "I couldn't finish setting that up: {e}. Want to try again?"
        )),
    }
}
