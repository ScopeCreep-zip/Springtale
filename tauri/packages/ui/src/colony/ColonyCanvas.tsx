import { For, Show, createSignal } from "solid-js";
import type { Component } from "solid-js";
import type {
  ColonyTree, ColonyAgent, ColonyConnection, ColonyFormation, ColonySelection,
} from "./types";
import { TREE_SPRITES, TREE_SIZES, ROLE_SPRITES, MUSHROOM_SPRITES, seeded } from "./types";

export interface ColonyCanvasProps {
  trees: ColonyTree[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  selection: ColonySelection;
  underground: boolean;
  onSelectTree: (id: string) => void;
  onSelectAgent: (id: string) => void;
  onSelectFormation: (id: string) => void;
  onClearSelection: () => void;
}

/**
 * Colony Canvas — spatial ecosystem rendering.
 *
 * Trees=connectors, springtails=agents, mycelium=pipelines.
 * Click to select elements. All positioning is percentage-based.
 */
export const ColonyCanvas: Component<ColonyCanvasProps> = (props) => {
  const getAgentPosition = (agent: ColonyAgent) => {
    const formation = props.formations.find((f) => f.members.includes(agent.id));
    if (formation) {
      return {
        x: formation.zone.x + seeded(agent.id + "fx", -8, 9),
        y: formation.zone.y + seeded(agent.id + "fy", -4, 5),
      };
    }
    const tree = props.trees.find((t) => t.id === agent.treeId);
    if (!tree) return { x: 50, y: 78 };
    return {
      x: tree.x + seeded(agent.id + "tx", -5, 6),
      y: tree.y + seeded(agent.id + "ty", 10, 16),
    };
  };

  const getMyceliumPath = (conn: ColonyConnection, width: number, height: number) => {
    const treeA = props.trees.find((t) => t.id === conn.a);
    const treeB = props.trees.find((t) => t.id === conn.b);
    if (!treeA || !treeB) return "";
    const x1 = (treeA.x * width) / 100;
    const y1 = (treeA.y * height) / 100;
    const x2 = (treeB.x * width) / 100;
    const y2 = (treeB.y * height) / 100;
    const key = conn.a + conn.b;
    const cx = seeded(key + "cx", -35, 36);
    const cy = seeded(key + "cy", 8, 45);
    return `M${x1},${y1} Q${(x1 + x2) / 2 + cx},${(y1 + y2) / 2 + cy} ${x2},${y2}`;
  };

  // Canvas dimensions for SVG viewBox
  const [canvasRef, setCanvasRef] = createSignal<HTMLDivElement>();
  const canvasWidth = () => canvasRef()?.offsetWidth ?? 1280;
  const canvasHeight = () => canvasRef()?.offsetHeight ?? 600;

  return (
    <div
      ref={(el) => setCanvasRef(el)}
      class={`relative h-full w-full ${props.underground ? "colony-underground" : ""}`}
      onClick={() => props.onClearSelection()}
    >
      {/* Ground texture */}
      <div
        class="pointer-events-none absolute inset-0 z-0"
        style={{
          background: "repeating-conic-gradient(from 0deg, transparent 0 89deg, rgba(50,42,25,.035) 89deg 90deg) 0 0 / 20px 20px",
        }}
      />

      {/* Leaf litter — decorative ground dots */}
      <For each={Array.from({ length: 45 }, (_, i) => i)}>
        {(i) => {
          const x = seeded("litter" + i + "x", 3, 97);
          const y = seeded("litter" + i + "y", 8, 92);
          const w = seeded("litter" + i + "w", 2, 4);
          const h = seeded("litter" + i + "h", 1, 3);
          const hue = seeded("litter" + i + "hue", 35, 50);
          const sat = seeded("litter" + i + "sat", 20, 40);
          const lgt = seeded("litter" + i + "lgt", 8, 16);
          const opacity = 0.12 + ((i * 7) % 20) / 100;
          return (
            <div
              class="colony-litter"
              style={{
                left: `${x}%`, top: `${y}%`,
                width: `${w}px`, height: `${h}px`,
                background: `hsl(${hue},${sat}%,${lgt}%)`,
                opacity: `${opacity}`,
              }}
            />
          );
        }}
      </For>

      {/* Mycelium layer — SVG pipeline paths */}
      <div class="pointer-events-none absolute inset-0 z-[2]">
        <svg
          viewBox={`0 0 ${canvasWidth()} ${canvasHeight()}`}
          class="h-full w-full"
          preserveAspectRatio="none"
        >
          <For each={props.connections}>
            {(conn) => {
              const pathData = () => getMyceliumPath(conn, canvasWidth(), canvasHeight());
              const hasActive = conn.pipes.some((p) => p.status === "active");
              const hasWarning = conn.pipes.some((p) => p.status === "warning");
              const strokeColor = hasActive ? "var(--color-mycelium-active)" : hasWarning ? "var(--color-mycelium-warning)" : "var(--color-mycelium)";
              const opacity = props.underground ? (hasActive ? 0.65 : 0.3) : (hasActive ? 0.25 : 0.1);
              const strokeWidth = props.underground ? 2.5 : 1.2;

              return (
                <>
                  <path
                    d={pathData()}
                    stroke={strokeColor}
                    class={`mycelium-path ${hasActive ? "is-active" : ""}`}
                    style={{ opacity: `${opacity}`, "stroke-width": `${strokeWidth}` }}
                  />
                  {/* Flow dots for active pipes */}
                  <For each={conn.pipes.filter((p) => p.status === "active")}>
                    {(pipe) => (
                      <circle r="2.5" fill={strokeColor} opacity="0.6">
                        <animateMotion
                          dur={`${2 + seeded(pipe.id + "dur", 0, 20) / 10}s`}
                          repeatCount="indefinite"
                          path={pathData()}
                          {...(pipe.dir === -1 ? { keyPoints: "1;0", keyTimes: "0;1", calcMode: "linear" } : {})}
                        />
                      </circle>
                    )}
                  </For>
                </>
              );
            }}
          </For>
        </svg>
      </div>

      {/* Formation zones */}
      <For each={props.formations}>
        {(formation) => (
          <div
            class={`colony-formation ${props.selection.id === formation.id && props.selection.type === "formation" ? "is-selected" : ""}`}
            style={{
              left: `calc(${formation.zone.x}% - 50px)`,
              top: `calc(${formation.zone.y}% - 30px)`,
              width: "100px", height: "60px",
            }}
            onClick={(e) => { e.stopPropagation(); props.onSelectFormation(formation.id); }}
          >
            <div
              class="colony-formation-ring"
              style={{ "border-color": formation.color }}
            />
            <div
              class="absolute flex items-center gap-1 whitespace-nowrap"
              style={{ top: "-18px", left: "50%", transform: "translateX(-50%)", "font-size": "6px" }}
            >
              <span style={{ color: formation.color }}>{formation.name}</span>
              <span
                class="px-1 font-bold"
                style={{
                  "font-size": "5px",
                  background: formation.color,
                  color: "var(--color-soil-deep)",
                }}
              >
                {formation.momentumLabel}
              </span>
            </div>
          </div>
        )}
      </For>

      {/* Trees (connectors) */}
      <For each={props.trees}>
        {(tree) => {
          const size = TREE_SIZES[tree.type];
          const spriteClass = TREE_SPRITES[tree.type];
          return (
            <div
              class={`colony-tree ${props.selection.id === tree.id && props.selection.type === "tree" ? "is-selected" : ""}`}
              style={{
                left: `calc(${tree.x}% - ${size.width / 2}px)`,
                top: `calc(${tree.y}% - ${size.height}px)`,
                width: `${size.width}px`,
                height: `${size.height + 16}px`,
              }}
              onClick={(e) => { e.stopPropagation(); props.onSelectTree(tree.id); }}
            >
              <div class={`pixel-sprite ${spriteClass}`} />
              <div
                class="absolute"
                style={{
                  bottom: "-4px", left: "50%", transform: "translateX(-50%)",
                  width: "4px", height: "4px",
                  background: tree.status === "active" ? "var(--color-status-ok)"
                    : tree.status === "paused" ? "var(--color-status-warn)"
                    : "var(--color-status-idle)",
                }}
              />
              <div
                class="absolute whitespace-nowrap text-text-dim"
                style={{
                  bottom: "-16px", left: "50%", transform: "translateX(-50%)",
                  "font-size": "5px",
                }}
              >
                {tree.label}
              </div>
            </div>
          );
        }}
      </For>

      {/* Mushrooms near active trees */}
      <For each={props.trees.filter((t) => t.status === "active")}>
        {(tree) => (
          <For each={Array.from({ length: seeded(tree.id + "mushcount", 1, 3) }, (_, i) => i)}>
            {(i) => {
              const spriteClass = MUSHROOM_SPRITES[seeded(tree.id + "mt" + i, 0, 3)];
              const ox = seeded(tree.id + "mx" + i, -4, 5);
              const oy = seeded(tree.id + "my" + i, 5, 12);
              return (
                <div
                  class="colony-mushroom"
                  style={{ left: `${tree.x + ox}%`, top: `${tree.y + oy}%` }}
                >
                  <div class={`pixel-sprite ${spriteClass}`} />
                </div>
              );
            }}
          </For>
        )}
      </For>

      {/* Agents (springtails) */}
      <For each={props.agents}>
        {(agent) => {
          const pos = () => getAgentPosition(agent);
          const spriteClass = ROLE_SPRITES[agent.role];
          const isDegraded = agent.fuel >= 20 && agent.fuel < 50;
          const isCritical = agent.fuel < 20;
          const isWarning = agent.status === "warn";
          const isSelected = props.selection.id === agent.id && props.selection.type === "agent";

          return (
            <div
              class={`colony-agent ${isSelected ? "is-selected" : ""} ${isDegraded ? "is-degraded" : ""} ${isCritical ? "is-critical" : ""} ${isWarning ? "is-warning" : ""}`}
              style={{
                left: `calc(${pos().x}% - 14px)`,
                top: `calc(${pos().y}% - 10px)`,
              }}
              onClick={(e) => { e.stopPropagation(); props.onSelectAgent(agent.id); }}
            >
              {/* Overhead info */}
              <div
                class="pointer-events-none absolute flex flex-col items-center gap-px"
                style={{ bottom: "100%", left: "50%", transform: "translateX(-50%)", "margin-bottom": "2px" }}
              >
                <span style={{ "font-size": "8px", filter: "drop-shadow(0 0 2px #000)" }}>
                  {agent.fuel < 20 ? "!" : agent.status === "warn" ? "!" : agent.status === "idle" ? "-" : "*"}
                </span>
                {/* Fuel bars — visible below 80% */}
                <Show when={agent.fuel < 80 || agent.hp < 80}>
                  <div class="flex gap-px">
                    <div class="colony-fuel-bar">
                      <div
                        class={`colony-fuel-fill ${agent.fuel > 50 ? "bg-status-ok" : agent.fuel > 20 ? "bg-status-warn" : "bg-status-error"}`}
                        style={{ width: `${agent.fuel}%` }}
                      />
                    </div>
                    <div class="colony-fuel-bar">
                      <div
                        class="colony-fuel-fill bg-role-scout"
                        style={{ width: `${agent.hp}%` }}
                      />
                    </div>
                  </div>
                </Show>
                <span class="text-text-dim" style={{ "font-size": "5px" }}>{agent.name}</span>
              </div>
              {/* Sprite */}
              <div class={`pixel-sprite ${spriteClass}`} />
            </div>
          );
        }}
      </For>
    </div>
  );
};
