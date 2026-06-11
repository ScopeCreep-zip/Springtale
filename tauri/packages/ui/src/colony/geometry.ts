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

/** Get an agent's position — always near its connector node. */
export function getAgentPosition(
  agent: ColonyAgent,
  nodes: ColonyNode[],
  connectorPositions: ConnectorPositions,
): { x: number; y: number } {
  if (!agent.connectorId) return { x: 50, y: 78 };
  const connectorPos = getConnectorPosition(agent.connectorId, nodes, connectorPositions);
  return {
    x: connectorPos.x + seeded(`${agent.id}tx`, -5, 6),
    y: connectorPos.y + seeded(`${agent.id}ty`, 10, 16),
  };
}

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
): { cx: number; cy: number; rx: number; ry: number } {
  const members = getFormationAgents(formation, agents);
  const positions = members.map((a) => getAgentPosition(a, nodes, connectorPositions));

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
