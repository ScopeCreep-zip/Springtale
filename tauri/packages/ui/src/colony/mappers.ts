/**
 * Colony data mappers — transform DashboardState into colony visual model.
 *
 * Shared by both desktop and dashboard App.tsx so the mapping
 * logic isn't duplicated. All agent state (role, fuel, activity)
 * comes from the backend via AgentState — no frontend inference.
 */
import type { ConnectorSchema, AgentState } from "@springtale/types";
import type { ConnectorStatus } from "../ResourceBar";
import type { RuleItem } from "../Roster";
import type { SwarmInfo } from "../SwarmCard";
import type {
  ColonyTree, ColonyAgent, ColonyConnection, ColonyFormation, ColonyPipe,
} from "./types";
import { seeded, MOMENTUM_COLORS } from "./types";

const TREE_TYPES = ["conifer", "deciduous", "shrub"] as const;
const MOMENTUM_LABELS = ["COLD", "WARM", "HOT", "FEVER"];

/** Map connectors to trees with deterministic positions. */
export function mapTrees(connectors: ConnectorStatus[]): ColonyTree[] {
  return connectors.map((c) => ({
    id: c.name,
    label: c.name,
    type: TREE_TYPES[seeded(c.name + "type", 0, 3)] ?? "deciduous",
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
      fuel: state?.fuel ?? (r.status === "enabled" ? 100 : 0),
      hp: 100,
      connectorId: r.connector ?? r.triggerType,
      task: state?.activity === "idle"
        ? "Idle"
        : `${r.triggerType} → ${state?.activity ?? "waiting"}`,
      status: r.status === "enabled" ? "ok" as const : "idle" as const,
      pipeline: r.triggerType,
      activity: state?.activity ?? "waiting",
    };
  });
}

/**
 * Build real connections from connector schemas.
 *
 * Connectors with triggers connect to connectors with actions.
 * This creates a natural spiderweb topology based on actual
 * connector capabilities, not a fake linear chain.
 */
export function mapConnections(
  trees: ColonyTree[],
  schemas: ConnectorSchema[],
  rules: RuleItem[],
): ColonyConnection[] {
  const conns: ColonyConnection[] = [];
  const seen = new Set<string>();

  // First: create connections from actual rules (trigger → action connector)
  for (const rule of rules) {
    const trigConn = rule.connector ?? rule.triggerType;
    const sourceTree = trees.find((t) => t.id === trigConn);
    if (!sourceTree) continue;

    // Find any other tree this rule could connect to
    // Rules connect their trigger connector to other connectors
    for (const destTree of trees) {
      if (destTree.id === sourceTree.id) continue;

      const key = [sourceTree.id, destTree.id].sort().join(":");
      if (seen.has(key)) {
        // Add pipe to existing connection
        const existing = conns.find((c) =>
          [c.a, c.b].sort().join(":") === key
        );
        if (existing) {
          existing.pipes.push({
            id: rule.id,
            dir: sourceTree.id === existing.a ? 1 : -1,
            status: rule.status === "enabled" ? "active" : "idle",
          });
        }
        break;
      }

      // Only connect if schemas show complementary capabilities
      const sourceSchema = schemas.find((s) => s.name === sourceTree.id);
      const destSchema = schemas.find((s) => s.name === destTree.id);
      if (!sourceSchema || !destSchema) continue;
      if (sourceSchema.triggers.length === 0 && destSchema.actions.length === 0) continue;

      seen.add(key);
      conns.push({
        a: sourceTree.id,
        b: destTree.id,
        pipes: [{
          id: rule.id,
          dir: 1,
          status: rule.status === "enabled" ? "active" : "idle",
        }],
      });
      break;
    }
  }

  return conns;
}

const TIER_TO_INDEX: Record<string, number> = {
  Cold: 0, Warming: 1, Hot: 2, Fever: 3,
};

/** Map swarms to formations. Momentum tier comes from backend. */
export function mapFormations(swarms: SwarmInfo[]): ColonyFormation[] {
  return swarms.map((s) => {
    const tierName = (s as { momentum_tier?: string }).momentum_tier ?? "Cold";
    const momentum = TIER_TO_INDEX[tierName] ?? 0;

    return {
      id: s.id,
      name: s.name,
      intent: s.intent.toUpperCase(),
      description: s.intent,
      momentum,
      momentumLabel: MOMENTUM_LABELS[momentum] ?? "COLD",
      color: MOMENTUM_COLORS[momentum] ?? "var(--color-momentum-cold)",
      members: s.members ?? [],
      zone: { x: seeded(s.id + "zx", 20, 80), y: seeded(s.id + "zy", 20, 60) },
    };
  });
}
