/**
 * Colony data mappers — transform DashboardState into colony visual model.
 *
 * Shared by both desktop and dashboard App.tsx so the mapping
 * logic isn't duplicated. All agent state (role, fuel, activity)
 * comes from the backend via AgentState — no frontend inference.
 */
import type { AgentState } from "@springtale/types";
import type { ConnectorStatus, RuleItem, SwarmInfo } from "../dashboard/model";
import type { CooperationEventEnvelope } from "../dashboard/types";
import type { ColonyAgent, ColonyFormation, ColonyNode } from "./types";
import { MOMENTUM_COLORS, seeded } from "./types";

/**
 * Phase H6 — map the backend `PacingPhase` enum (snake_case-serialized
 * via serde) into the slugged CSS-attribute value the colony stylesheet
 * keys on. Returning `undefined` lets the absence of pacing-phase data
 * fall back to the default sprite appearance.
 */
function slugPacingPhase(raw: string): ColonyFormation["pacingPhase"] | undefined {
  switch (raw) {
    case "preparation":
      return "prep";
    case "active":
      return "active";
    case "peak":
      return "peak";
    case "recovery":
      return "recovery";
    case "disruption":
      return "disrupted";
    default:
      return undefined;
  }
}

const NODE_TYPES = ["conifer", "deciduous", "shrub"] as const;
const MOMENTUM_LABELS = ["COLD", "WARM", "HOT", "FEVER"];

/** Map connectors to nodes with deterministic positions. */
export function mapNodes(connectors: ConnectorStatus[]): ColonyNode[] {
  return connectors.map((c) => ({
    id: c.name,
    label: c.name,
    type: NODE_TYPES[seeded(`${c.name}type`, 0, 3)] ?? "deciduous",
    x: seeded(`${c.name}x`, 8, 92),
    y: seeded(`${c.name}y`, 15, 70),
    status: c.enabled ? ("active" as const) : ("idle" as const),
  }));
}

/** Map backend agent states to colony agents. All business logic
 *  (role, fuel, activity, autonomy) comes from the backend. */
export function mapAgents(rules: RuleItem[], agentStates: AgentState[] = []): ColonyAgent[] {
  return rules.map((r) => {
    // Find matching backend state for this rule
    const state = agentStates.find((s) => s.rule_id === r.id);

    return {
      id: r.id,
      name: r.name,
      role: (state?.role ?? "worker") as ColonyAgent["role"],
      autonomy: state?.autonomy ?? 1,
      autonomyLabel: state?.autonomy_label ?? "SUGGEST",
      fuel: state?.fuel ?? 0,
      fuelStatus: (state?.fuel_status ?? "ok") as ColonyAgent["fuelStatus"],
      hp: state ? Math.round(state.liveness * 100) : 100,
      connectorId: r.connector ?? r.triggerType,
      actionConnectorId: state?.action_connector ?? null,
      task: state?.task_display ?? "",
      status: r.status === "enabled" ? ("ok" as const) : ("idle" as const),
      pipeline: r.triggerType,
      activity: state?.activity,
      attentionLoad: state?.attention_load ?? 0,
      liveness: state?.liveness ?? 1,
      healthState: state?.health_state ?? "healthy",
    };
  });
}

// Connection graph moved to backend: springtale_runtime::operations::canvas::compute_connections()
// Frontend receives pre-computed connections via provider.getConnections().

const TIER_TO_INDEX: Record<string, number> = {
  Cold: 0,
  Warming: 1,
  Hot: 2,
  Fever: 3,
};

/** Map swarms to formations. Momentum tier + label come from backend.
 *  Optionally folds the cooperation events stream so each formation's
 *  `pacingPhase` reflects the most-recent `pacing_phase_changed` event. */
export function mapFormations(
  swarms: SwarmInfo[],
  cooperationEvents: CooperationEventEnvelope[] = [],
): ColonyFormation[] {
  // Most-recent-first; first `pacing_phase_changed` per formation wins.
  const latestPacing = new Map<string, string>();
  // Most-recent `cascade_hit` per formation, with its event timestamp, so we
  // can gate the canvas glow on recency (see CASCADE_RECENT_MS below).
  const latestCascade = new Map<string, { streak: number; at: number }>();
  // The colony's own "now" = the newest event timestamp in the buffer
  // (events are stored most-recent-first). We measure cascade recency
  // against this, NOT wall-clock — so the glow is driven purely by real
  // event data and expires as the real event timeline advances.
  let timelineNow = 0;
  for (const env of cooperationEvents) {
    const ts = Date.parse(env.at);
    if (!Number.isNaN(ts) && ts > timelineNow) timelineNow = ts;
    if (env.event.kind === "pacing_phase_changed" && !latestPacing.has(env.event.formation_id)) {
      latestPacing.set(env.event.formation_id, env.event.to);
    }
    if (env.event.kind === "cascade_hit" && !latestCascade.has(env.event.formation_id)) {
      latestCascade.set(env.event.formation_id, {
        streak: env.event.streak,
        at: Number.isNaN(ts) ? 0 : ts,
      });
    }
  }

  return swarms.map((s) => {
    const momentum = TIER_TO_INDEX[s.momentum_tier ?? "Cold"] ?? 0;
    const rawPhase = latestPacing.get(s.id);
    // Show the cascade glow only while the hit is recent on the colony's
    // own timeline. Once newer events push past CASCADE_RECENT_MS without a
    // fresh hit, this resolves to undefined and the glow clears.
    const cascade = latestCascade.get(s.id);
    const cascadeStreak =
      cascade && timelineNow - cascade.at <= CASCADE_RECENT_MS ? cascade.streak : undefined;

    return {
      id: s.id,
      name: s.name,
      intent: s.intent.toUpperCase(),
      description: s.intent,
      momentum,
      momentumLabel: s.momentum_label ?? MOMENTUM_LABELS[momentum] ?? "COLD",
      color: MOMENTUM_COLORS[momentum] ?? "var(--color-momentum-cold)",
      members: s.members ?? [],
      zone: { x: seeded(`${s.id}zx`, 20, 80), y: seeded(`${s.id}zy`, 20, 60) },
      status: s.status ?? "draft",
      rallyTokens: s.rally_tokens ?? 0,
      rallyMax: s.rally_max ?? 0,
      guardStatus: s.guard_status,
      pacingPhase: rawPhase ? slugPacingPhase(rawPhase) : undefined,
      cascadeStreak,
    };
  });
}

/** A cascade_hit is shown as "active" for this long after it occurs,
 *  measured against the colony's latest event timestamp (not wall-clock). */
const CASCADE_RECENT_MS = 4000;
