/**
 * Colony data mappers — transform DashboardState into colony visual model.
 *
 * Shared by both desktop and dashboard App.tsx so the mapping
 * logic isn't duplicated. All agent state (role, fuel, activity)
 * comes from the backend via AgentState — no frontend inference.
 */
import type { ConnectorSchema, AgentState } from "@springtale/types";
import type { ConnectorStatus, RuleItem, SwarmInfo } from "../dashboard/model";
import type {
  ColonyNode, ColonyAgent, ColonyConnection, ColonyFormation, ColonyPipe,
} from "./types";
import { seeded, MOMENTUM_COLORS } from "./types";

const NODE_TYPES = ["conifer", "deciduous", "shrub"] as const;
const MOMENTUM_LABELS = ["COLD", "WARM", "HOT", "FEVER"];

/** Map connectors to nodes with deterministic positions. */
export function mapNodes(connectors: ConnectorStatus[]): ColonyNode[] {
  return connectors.map((c) => ({
    id: c.name,
    label: c.name,
    type: NODE_TYPES[seeded(c.name + "type", 0, 3)] ?? "deciduous",
    x: seeded(c.name + "x", 8, 92),
    y: seeded(c.name + "y", 15, 70),
    status: c.enabled ? "active" as const : "idle" as const,
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
      hp: 100,
      connectorId: r.connector ?? r.triggerType,
      task: state?.task_display ?? "",
      status: r.status === "enabled" ? "ok" as const : "idle" as const,
      pipeline: r.triggerType,
      activity: state?.activity ?? "waiting",
    };
  });
}

// Connection graph moved to backend: springtale_runtime::operations::canvas::compute_connections()
// Frontend receives pre-computed connections via provider.getConnections().

const TIER_TO_INDEX: Record<string, number> = {
  Cold: 0, Warming: 1, Hot: 2, Fever: 3,
};

/** Map swarms to formations. Momentum tier + label come from backend. */
export function mapFormations(swarms: SwarmInfo[]): ColonyFormation[] {
  return swarms.map((s) => {
    const momentum = TIER_TO_INDEX[s.momentum_tier ?? "Cold"] ?? 0;

    return {
      id: s.id,
      name: s.name,
      intent: s.intent.toUpperCase(),
      description: s.intent,
      momentum,
      momentumLabel: s.momentum_label ?? MOMENTUM_LABELS[momentum] ?? "COLD",
      color: MOMENTUM_COLORS[momentum] ?? "var(--color-momentum-cold)",
      members: s.members ?? [],
      zone: { x: seeded(s.id + "zx", 20, 80), y: seeded(s.id + "zy", 20, 60) },
    };
  });
}
