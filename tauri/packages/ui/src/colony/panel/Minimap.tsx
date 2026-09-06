import type { Component } from "solid-js";
import { For } from "solid-js";
import type { ConnectorPositions } from "../geometry";
import { getAgentPosition, getFormationBounds } from "../geometry";
import type {
  ColonyAgent,
  ColonyConnection,
  ColonyFormation,
  ColonyNode,
  ColonySelection,
} from "../types";
import { ROLE_COLORS } from "../types";

export const Minimap: Component<{
  nodes: ColonyNode[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  selection: ColonySelection;
  connectorPositions: ConnectorPositions;
}> = (props) => {
  const connectorPos = (id: string) => {
    const tree = props.nodes.find((t) => t.id === id);
    if (!tree) return { x: 50, y: 50 };
    return props.connectorPositions[id] ?? { x: tree.x, y: tree.y };
  };

  return (
    <div class="colony-minimap">
      {/* Formation zones — computed from member positions via shared geometry */}
      <For each={props.formations}>
        {(f) => {
          const bounds = () =>
            getFormationBounds(f, props.agents, props.nodes, props.connectorPositions);
          return (
            <div
              class="colony-minimap-formation absolute rounded-[40%] opacity-30"
              style={{
                left: `${bounds().cx - bounds().rx}%`,
                top: `${bounds().cy - bounds().ry}%`,
                width: `${bounds().rx * 2}%`,
                "--colony-h": `${bounds().ry * 2}%`,
                "--colony-color": f.color,
              }}
            />
          );
        }}
      </For>

      {/* Trees — respect drag positions */}
      <For each={props.nodes}>
        {(tree) => {
          const pos = () => connectorPos(tree.id);
          return (
            <div
              class="colony-minimap-tree"
              data-status={tree.status}
              style={{
                left: `${pos().x - 1}%`,
                top: `${pos().y - 1}%`,
              }}
            />
          );
        }}
      </For>

      {/* Connections — respect drag positions */}
      <For each={props.connections}>
        {(conn) => {
          const a = () => connectorPos(conn.a);
          const b = () => connectorPos(conn.b);
          const dx = () => b().x - a().x;
          const dy = () => b().y - a().y;
          const length = () => Math.sqrt(dx() * dx() + dy() * dy());
          const angle = () => (Math.atan2(dy(), dx()) * 180) / Math.PI;
          const hasActive = conn.pipes.some((p) => p.status === "active");
          return (
            <div
              class="colony-minimap-line"
              style={{
                left: `${a().x}%`,
                top: `${a().y}%`,
                width: `${length()}%`,
                transform: `rotate(${angle()}deg)`,
              }}
              classList={{ "is-active": hasActive }}
            />
          );
        }}
      </For>

      {/* Agents — via shared geometry, respects drag positions */}
      <For each={props.agents}>
        {(agent) => {
          const pos = () => getAgentPosition(agent, props.nodes, props.connectorPositions);
          const isSelected = () => props.selection.id === agent.id;
          return (
            <div
              class="colony-minimap-dot"
              classList={{ "is-selected": isSelected() }}
              style={{
                left: `${pos().x}%`,
                top: `${pos().y}%`,
                "--colony-bg": ROLE_COLORS[agent.role] ?? "var(--color-text-secondary)",
              }}
            />
          );
        }}
      </For>
    </div>
  );
};

// ── Detail Panel ─────────────────────────────────────────
