/**
 * Agent activity (ALIGNMENT-PLAN 3.2 — "live means live").
 *
 * An agent's activity is what it said: the newest unexpired utterance in the
 * cooperation ring, expiring on the colony tick clock (`seq + ttl_ticks > now`).
 * There is no separate activity event and no client-side decay timer; the value
 * is derived, never stored. `AgentState.activity` from the backend
 * (`compute_activity` in `operations/agent.rs`) is the same derivation done at
 * fetch time and only seeds an agent until its first utterance arrives.
 */

import type { ColonyAgent } from "../colony/types";
import type { Utterance } from "./types";

/** Said while silent: the canvas vocabulary's resting state. */
export const SILENT_ACTIVITY = "listening";

/**
 * Formation members match through the `agent_id → connector` map from
 * `getFormation`; solo agents through `rule_id`, which `trigger_dispatch`
 * stamps on its utterances; a formation-level utterance lands on the
 * synthetic agent whose id is the formation's.
 */
export function agentMatches(
  u: Utterance,
  agent: ColonyAgent,
  agentToConnector: Record<string, string>,
): boolean {
  if (u.rule_id && u.rule_id === agent.id) return true;
  if (!u.agent && !u.rule_id && u.formation_id === agent.id) return true;
  return !!u.agent && !!agent.connectorId && agentToConnector[u.agent] === agent.connectorId;
}

/**
 * The newest unexpired utterance's `utter` for this agent (the canvas
 * `is-*` vocabulary); `listening` once everything it said has expired; the
 * backend's fetched `activity` only before any utterance of its has arrived.
 * `utterances` is the ring, newest first.
 */
export function activityOf(
  agent: ColonyAgent,
  utterances: Utterance[],
  now: number,
  agentToConnector: Record<string, string>,
): string {
  let heard = false;
  for (const u of utterances) {
    if (!agentMatches(u, agent, agentToConnector)) continue;
    if (u.seq + u.ttl_ticks > now) return u.utterance.utter;
    heard = true;
  }
  return heard ? SILENT_ACTIVITY : (agent.activity ?? SILENT_ACTIVITY);
}
