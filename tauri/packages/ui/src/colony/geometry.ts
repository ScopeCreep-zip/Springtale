/**
 * Colony geometry — shared position calculations for canvas + minimap.
 *
 * Pure functions. No reactivity, no component code. Both ColonyCanvas
 * and the Minimap import from here so position logic is never duplicated.
 *
 * connectorPositions always flows through — this is how drag overrides
 * propagate to both the main canvas and the minimap.
 */
import type { ColonyAgent, ColonyFormation, ColonyNode } from "./types";
import { seeded } from "./types";

export type ConnectorPositions = Record<string, { x: number; y: number }>;

/** Get a node's position, respecting drag overrides. */
export function getConnectorPosition(
  connectorId: string,
  nodes: ColonyNode[],
  connectorPositions: ConnectorPositions,
): { x: number; y: number } {
  const node = nodes.find((n) => n.id === connectorId);
  if (!node) return { x: 50, y: 50 };
  return connectorPositions[connectorId] ?? { x: node.x, y: node.y };
}

/**
 * Get an agent's position (plan 3.5 — springtails move toward what they do).
 *
 * Three rules, in order:
 *  - no connector: parked on the floor, spread by id so they never stack;
 *  - firing at a `RunConnector` target: part-way along the mycelium toward
 *    that tree, so the walk itself is the signal;
 *  - otherwise: at home, standing off from the trunk by an amount that grows
 *    with the agent's attention load, so a busy tree's agents fan out.
 *
 * `activity` is the live value derived from the cooperation ring
 * (`activityOf`); it falls back to the backend's fetch-time `agent.activity`
 * so callers with no ring (the minimap) still get a sane position.
 */
export function getAgentPosition(
  agent: ColonyAgent,
  nodes: ColonyNode[],
  connectorPositions: ConnectorPositions,
  activity: string | undefined = agent.activity,
): { x: number; y: number } {
  if (!agent.connectorId) {
    // The plan spreads these on y alone; eight rows collide as soon as a
    // colony has a handful of unattached agents (its own test — "two
    // unconnected agents do not overlap" — catches it), so the spread is
    // two-dimensional. Same seeded, deterministic scheme.
    return { x: 50 + seeded(`${agent.id}ux`, -12, 13), y: 78 + seeded(`${agent.id}u`, 0, 8) };
  }
  const home = getConnectorPosition(agent.connectorId, nodes, connectorPositions);
  const standoff = 10 + 6 * agent.attentionLoad;
  if (activity === "firing" && agent.actionConnectorId) {
    const to = getConnectorPosition(agent.actionConnectorId, nodes, connectorPositions);
    const t = WALK_FRACTION;
    return { x: home.x + (to.x - home.x) * t, y: home.y + (to.y - home.y) * t };
  }
  return { x: home.x + seeded(`${agent.id}tx`, -5, 6), y: home.y + standoff };
}

/** How far along the mycelium a firing agent walks toward its action target. */
const WALK_FRACTION = 0.35;

/** Get all agents that belong to a formation (matched by connector name). */
export function getFormationAgents(
  formation: ColonyFormation,
  agents: ColonyAgent[],
): ColonyAgent[] {
  return agents.filter((a) => a.connectorId && formation.members.includes(a.connectorId));
}

/**
 * Compute the bounding ellipse for a formation's member agents.
 *
 * Uses axis-aligned bounding box + padding (AoE/StarCraft pattern).
 * Returns percentage coordinates matching the canvas coordinate system.
 */
export function getFormationBounds(
  formation: ColonyFormation,
  agents: ColonyAgent[],
  nodes: ColonyNode[],
  connectorPositions: ConnectorPositions,
  activityFor?: (agent: ColonyAgent) => string,
): { cx: number; cy: number; rx: number; ry: number } {
  const members = getFormationAgents(formation, agents);
  // The ellipse follows the members, so it must use the same live activity
  // the canvas draws them with — otherwise a firing agent walks out of its
  // own zone.
  const positions = members.map((a) =>
    getAgentPosition(a, nodes, connectorPositions, activityFor?.(a)),
  );

  if (positions.length === 0) {
    // No members — fall back to zone center with minimum size
    return { cx: formation.zone.x, cy: formation.zone.y, rx: 5, ry: 4 };
  }

  const xs = positions.map((p) => p.x);
  const ys = positions.map((p) => p.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);

  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;

  // Padding: agent sprite ~20px ≈ 2% on typical canvas, plus margin
  const padX = 4;
  const padY = 4;
  const rx = Math.max((maxX - minX) / 2 + padX, 5);
  const ry = Math.max((maxY - minY) / 2 + padY, 4);

  return { cx, cy, rx, ry };
}
