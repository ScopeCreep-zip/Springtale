//! One helper every event site calls. Resolves the def, applies Stardew's
//! `blockedIntervalBeforeEmote` per `(agent, kind)`, then speaks to two
//! audiences: the formation (bus, `Speech`/`Burst` only — Cohn 2013) and
//! the observer stream (every carrier, thoughts included).

use std::collections::HashMap;

use tokio::sync::broadcast;

use crate::cadence::AgentId;
use crate::comms::{BroadcastTrigger, FormationBus, StateBroadcastMsg, StateMessage};
use crate::events::{self, CooperationEvent, CooperationEventEnvelope};
use crate::tick::TickId;
use crate::types::FormationId;

use super::defs::UtteranceDefs;
use super::types::{Utterance, UtteranceKind};

/// Block map key: who said it (`None` = the formation) and the def name.
pub type LastUttered = HashMap<(Option<AgentId>, &'static str), TickId>;

/// Everything `utter` needs, borrowed as disjoint fields so it works
/// inside `for member in &mut formation.members`.
pub struct UtterCtx<'a> {
    pub formation_id: FormationId,
    pub bus: &'a FormationBus,
    pub defs: &'a UtteranceDefs,
    pub last_uttered: &'a mut LastUttered,
    pub tick: TickId,
    pub tx: Option<&'a broadcast::Sender<CooperationEventEnvelope>>,
}

/// Say `kind` as `agent` (or as the formation when `None`). Returns the
/// utterance if it was not blocked by the def's `block_ticks`.
pub fn utter(
    ctx: &mut UtterCtx<'_>,
    agent: Option<AgentId>,
    kind: UtteranceKind,
) -> Option<Utterance> {
    let name = kind.name();
    let def = ctx.defs.get(name)?;
    // Stardew's blockedIntervalBeforeEmote, per (agent, kind).
    if let Some(last) = ctx.last_uttered.get(&(agent, name))
        && ctx.tick.delta(*last) < u64::from(def.block_ticks)
    {
        return None;
    }
    ctx.last_uttered.insert((agent, name), ctx.tick);
    let u = Utterance {
        formation_id: Some(ctx.formation_id),
        agent,
        rule_id: None,
        utterance: kind,
        carrier: def.carrier,
        shape: def.shape,
        tone: def.tone,
        seq: ctx.tick,
        ttl_ticks: def.ttl_ticks,
        glyph_frames: def.frames.clone(),
        mirror_rtl: def.mirror_rtl,
        label_key: def.label_key.clone(),
    };
    // Audience 1: the formation. Cohn: speech is perceived by others in the scene.
    if def.carrier.heard_by_peers()
        && let Some(source) = agent
    {
        ctx.bus.broadcast_state(StateBroadcastMsg {
            source,
            trigger: BroadcastTrigger::Utterance(u.clone()),
            message: StateMessage {
                content: name.to_owned(),
                severity: def.tone.severity(),
            },
        });
    }
    // Audience 2: the observer. Every utterance, thoughts included.
    events::emit(ctx.tx, CooperationEvent::from(u.clone()));
    Some(u)
}
