import { For, Show, createSignal, createEffect } from "solid-js";
import { createElementSize } from "@solid-primitives/resize-observer";
import type { Component } from "solid-js";
import type {
  ColonyNode, ColonyAgent, ColonyConnection, ColonyFormation, ColonySelection,
} from "./types";
import type { EventItem } from "../dashboard/model";
import { NODE_SPRITES, NODE_SIZES, ROLE_SPRITES, MUSHROOM_SPRITES, seeded } from "./types";
import { getConnectorPosition, getAgentPosition, getFormationAgents, getFormationBounds } from "./geometry";
import type { AvailableConnector, ConnectorSchema } from "@springtale/types";

export interface ColonyCanvasProps {
  nodes: ColonyNode[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  events: EventItem[];
  selection: ColonySelection;
  underground: boolean;
  connectorPositions: Record<string, { x: number; y: number }>;
  onSelectConnector: (id: string) => void;
  onSelectAgent: (id: string) => void;
  onSelectFormation: (id: string) => void;
  onClearSelection: () => void;
  onConnectorDrag: (id: string, x: number, y: number) => void;
  onHatch?: () => void;
  // OOBE TeamBuilder props
  availableConnectors?: AvailableConnector[];
  connectorSchemas?: ConnectorSchema[];
  onSetupConnector?: (name: string) => void;
  onParseRule?: (intent: string) => Promise<Record<string, unknown>>;
}

// ── Simlish vocabulary — maps to real agent activity states ──
const SIMLISH_FIRING = ["zib!", "klik!", "pip!"];
const SIMLISH_ERROR = ["vrm!", "nrt!", "!!"];
const SIMLISH_WAITING = ["hrmm...", "~", "..."];
const SIMLISH_ACTIVE = ["bzzk!", "snrf...", "pip!"];

function getSimlish(activity: string): { text: string; type: "normal" | "urgent" } | null {
  switch (activity) {
    case "firing": return { text: SIMLISH_FIRING[Math.floor(Math.random() * SIMLISH_FIRING.length)] ?? "zib!", type: "normal" };
    case "error": return { text: SIMLISH_ERROR[Math.floor(Math.random() * SIMLISH_ERROR.length)] ?? "vrm!", type: "urgent" };
    case "waiting": return { text: SIMLISH_WAITING[Math.floor(Math.random() * SIMLISH_WAITING.length)] ?? "...", type: "normal" };
    case "active": return { text: SIMLISH_ACTIVE[Math.floor(Math.random() * SIMLISH_ACTIVE.length)] ?? "bzzk!", type: "normal" };
    default: return null;
  }
}

/**
 * Colony Canvas — spatial diorama rendering.
 *
 * Nodes=connectors, agents=porters, strands=pipelines.
 * Click to select, drag to reposition nodes.
 * Agent simlish bubbles reflect real activity state.
 */
export const ColonyCanvas: Component<ColonyCanvasProps> = (props) => {
  // ── Position helpers (delegate to shared geometry module) ──
  const getConnectorPos = (id: string) =>
    getConnectorPosition(id, props.nodes, props.connectorPositions);

  const agentPos = (agent: ColonyAgent) =>
    getAgentPosition(agent, props.nodes, props.connectorPositions);

  const getMyceliumPath = (conn: ColonyConnection, width: number, height: number) => {
    const posA = getConnectorPos(conn.a);
    const posB = getConnectorPos(conn.b);
    const x1 = (posA.x * width) / 100;
    const y1 = (posA.y * height) / 100;
    const x2 = (posB.x * width) / 100;
    const y2 = (posB.y * height) / 100;
    const key = conn.a + conn.b;
    const cx = seeded(key + "cx", -35, 36);
    const cy = seeded(key + "cy", 8, 45);
    return `M${x1},${y1} Q${(x1 + x2) / 2 + cx},${(y1 + y2) / 2 + cy} ${x2},${y2}`;
  };

  // ── Agent movement tracking ────────────────────────────
  // Track previous positions so we can detect movement and show walking state.
  // When an agent's target position changes (reassigned to different tree,
  // tree dragged), the CSS transition animates the move and we show
  // the is-walking class until transitionend fires.
  const [walkingAgents, setWalkingAgents] = createSignal<Set<string>>(new Set());
  let prevPositions: Record<string, { x: number; y: number }> = {};

  createEffect(() => {
    const nextWalking = new Set<string>();
    for (const agent of props.agents) {
      const pos = agentPos(agent);
      const prev = prevPositions[agent.id];
      if (prev && (Math.abs(prev.x - pos.x) > 0.5 || Math.abs(prev.y - pos.y) > 0.5)) {
        nextWalking.add(agent.id);
      }
      prevPositions[agent.id] = pos;
    }
    if (nextWalking.size > 0) {
      setWalkingAgents(nextWalking);
      // Clear walking state after CSS transition completes (800ms matches transition duration)
      setTimeout(() => setWalkingAgents(new Set<string>()), 850);
    }
  });

  // ── Agent activity state — comes from backend ──────────
  const getAgentActivity = (agent: ColonyAgent): "firing" | "error" | "active" | "waiting" | "idle" => {
    return (agent.activity ?? "waiting") as "firing" | "error" | "active" | "waiting" | "idle";
  };

  // ── Simlish bubbles (event-driven, not random) ─────────
  const [bubbles, setBubbles] = createSignal<Record<string, { text: string; type: string; key: number }>>({});
  let bubbleKey = 0;
  let prevActivities: Record<string, string> = {};

  createEffect(() => {
    for (const agent of props.agents) {
      const act = getAgentActivity(agent);
      if (prevActivities[agent.id] !== act && act !== "idle") {
        const simlish = getSimlish(act);
        if (simlish) {
          const key = ++bubbleKey;
          setBubbles((prev) => ({ ...prev, [agent.id]: { ...simlish, key } }));
          setTimeout(() => setBubbles((prev) => {
            const next = { ...prev };
            if (next[agent.id]?.key === key) delete next[agent.id];
            return next;
          }), 3000);
        }
      }
      prevActivities[agent.id] = act;
    }
  });

  // ── Canvas dimensions (reactive via ResizeObserver) ────
  // Per React Flow pattern: use ResizeObserver to track container
  // dimensions reactively. Without this, the SVG viewBox uses stale
  // defaults because SolidJS evaluates before DOM layout completes.
  const [canvasRef, setCanvasRef] = createSignal<HTMLDivElement>();
  const canvasSize = createElementSize(canvasRef);
  const canvasWidth = () => canvasSize.width ?? 1280;
  const canvasHeight = () => canvasSize.height ?? 600;

  return (
    <div
      ref={(el) => setCanvasRef(el)}
      class={`relative h-full w-full ${props.underground ? "colony-underground" : ""}`}
      onClick={(e) => {
        // Event delegation (v8 reference pattern, lines 1069-1084).
        // Single handler on canvas root — uses closest() to find the
        // nearest interactive ancestor. Works even when the click lands
        // on a tiny child element (1px sprite, overhead text, etc.).
        const target = (e.target as HTMLElement).closest?.(
          "[data-agent-id], [data-connector-id], [data-formation-id]"
        );
        if (target instanceof HTMLElement) {
          if (target.dataset.agentId) {
            props.onSelectAgent(target.dataset.agentId);
            return;
          }
          if (target.dataset.connectorId) {
            props.onSelectConnector(target.dataset.connectorId);
            return;
          }
          if (target.dataset.formationId) {
            props.onSelectFormation(target.dataset.formationId);
            return;
          }
        }
        // No interactive element found — check formation interior
        const rect = canvasRef()?.getBoundingClientRect();
        if (rect) {
          const pctX = ((e.clientX - rect.left) / rect.width) * 100;
          const pctY = ((e.clientY - rect.top) / rect.height) * 100;
          for (const f of props.formations) {
            const b = getFormationBounds(f, props.agents, props.nodes, props.connectorPositions);
            if (pctX >= b.cx - b.rx && pctX <= b.cx + b.rx &&
                pctY >= b.cy - b.ry && pctY <= b.cy + b.ry) {
              props.onSelectFormation(f.id);
              return;
            }
          }
        }
        props.onClearSelection();
      }}
    >
      {/* Ground texture */}
      <div class="colony-ground pointer-events-none absolute inset-0 z-0" />

      {/* OOBE — minimal hint when canvas is empty. The TeamBuilder
          renders as a shell overlay (same layer as settings/vault),
          not inside the canvas. */}
      <Show when={props.nodes.length === 0 && props.agents.length === 0}>
        <div class="absolute inset-0 z-10 flex flex-col items-center justify-center text-center">
          <div style={{ width: "42px", height: "30px", position: "relative" }}>
            <div class="pixel-sprite sprite-tree-shrub" style={{ transform: "scale(6)" }} />
          </div>
          <p class="colony-text-md mt-12 font-bold text-text-primary">Your network awaits</p>
          <p class="colony-text-xs mt-2 text-text-secondary">
            Press BUILD BOT/TEAM or click below to begin
          </p>
          <button
            class="colony-command-btn mt-6 colony-text-sm"
            style={{ "border-color": "var(--color-status-ok)", padding: "8px 24px" }}
            onClick={(e) => { e.stopPropagation(); props.onHatch?.(); }}
          >
            BUILD BOT/TEAM
          </button>
        </div>
      </Show>

      {/* Board texture — decorative ground scatter */}
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

      {/* Strand layer — SVG pipeline paths */}
      <div class="pointer-events-none absolute inset-0 z-[2]">
        <svg
          viewBox={`0 0 ${canvasWidth()} ${canvasHeight()}`}
          class="h-full w-full"
          preserveAspectRatio="none"
        >
          <defs>
            <filter id="strand-glow">
              <feGaussianBlur in="SourceGraphic" stdDeviation="2" result="blur"/>
              <feMerge>
                <feMergeNode in="blur"/>
                <feMergeNode in="SourceGraphic"/>
              </feMerge>
            </filter>
          </defs>
          <For each={props.connections}>
            {(conn) => {
              const pathData = () => getMyceliumPath(conn, canvasWidth(), canvasHeight());
              const hasActive = conn.pipes.some((p) => p.status === "active");
              const hasWarning = conn.pipes.some((p) => p.status === "warning");
              const strokeColor = hasActive ? "var(--color-mycelium-active)" : hasWarning ? "var(--color-mycelium-warning)" : "var(--color-mycelium)";
              const opacity = props.underground ? (hasActive ? 0.8 : 0.45) : (hasActive ? 0.45 : 0.2);
              const strokeWidth = props.underground ? 2.5 : 1.5;

              return (
                <>
                  <path
                    d={pathData()}
                    stroke={strokeColor}
                    class={`mycelium-path ${hasActive ? "is-active" : ""}`}
                    style={{ opacity: `${opacity}`, "stroke-width": `${strokeWidth}` }}
                  />
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

      {/* Formation zones — SVG ellipse with pointer-events: stroke.
          Per MDN: HTML divs can only do pointer-events auto/none (whole element).
          SVG supports pointer-events: stroke — only the ring border is clickable,
          clicks on the interior pass through to agents/nodes underneath. */}
      <For each={props.formations}>
        {(formation) => {
          const bounds = () => getFormationBounds(
            formation, props.agents, props.nodes, props.connectorPositions,
          );
          const isSelected = () => props.selection.id === formation.id && props.selection.type === "formation";
          return (
            <>
              {/* Ring — SVG ellipse, stroke-only hit testing */}
              <svg
                class="colony-formation"
                style={{
                  left: `calc(${bounds().cx}% - ${bounds().rx}% - 8px)`,
                  top: `calc(${bounds().cy}% - ${bounds().ry}% - 8px)`,
                  width: `calc(${bounds().rx * 2}% + 16px)`,
                  height: `calc(${bounds().ry * 2}% + 16px)`,
                  overflow: "visible",
                }}
              >
                <ellipse
                  cx="50%"
                  cy="50%"
                  rx="calc(50% - 4px)"
                  ry="calc(50% - 4px)"
                  fill="none"
                  stroke={formation.color}
                  stroke-width={isSelected() ? 2.5 : 1.5}
                  stroke-dasharray={isSelected() ? "none" : "4 6"}
                  pointer-events="stroke"
                  cursor="pointer"
                  onClick={(e) => { e.stopPropagation(); props.onSelectFormation(formation.id); }}
                />
              </svg>
              {/* Label — positioned above the ring */}
              <div
                class="absolute z-[3] flex items-center gap-1 whitespace-nowrap"
                style={{
                  left: `${bounds().cx}%`,
                  top: `calc(${bounds().cy}% - ${bounds().ry}% - 18px)`,
                  transform: "translateX(-50%)",
                  "font-size": "6px",
                  "pointer-events": "auto",
                  cursor: "pointer",
                }}
                data-formation-id={formation.id}
              >
                <span style={{ color: formation.color, cursor: "pointer" }}>{formation.name}</span>
                <span
                  class="px-1 font-bold"
                  style={{ "font-size": "5px", background: formation.color, color: "var(--color-soil-deep)", cursor: "pointer" }}
                >
                  {formation.momentumLabel}
                </span>
              </div>
            </>
          );
        }}
      </For>

      {/* Nodes (connectors) — click to select, drag to reposition */}
      <For each={props.nodes}>
        {(node) => {
          const size = NODE_SIZES[node.type] ?? { width: 36, height: 44 };
          const spriteClass = NODE_SPRITES[node.type] ?? "sprite-tree-deciduous";
          const pos = () => getConnectorPos(node.id);

          let dragStart: { cx: number; cy: number; ox: number; oy: number } | null = null;
          let wasDragged = false;

          // React Flow pattern: capture pointer immediately on pointerdown
          // so pointerup always fires on this element even if cursor drifts.
          // Click vs drag resolved in pointerup by checking distance threshold.
          const onPointerDown = (e: PointerEvent) => {
            e.stopPropagation();
            wasDragged = false;
            dragStart = { cx: e.clientX, cy: e.clientY, ox: pos().x, oy: pos().y };
            (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
          };
          const onPointerMove = (e: PointerEvent) => {
            if (!dragStart) return;
            if (Math.abs(e.clientX - dragStart.cx) > 3 || Math.abs(e.clientY - dragStart.cy) > 3) wasDragged = true;
            if (!wasDragged) return;
            const parent = (e.currentTarget as HTMLElement).parentElement;
            if (!parent) return;
            const rect = parent.getBoundingClientRect();
            const pctX = ((e.clientX - dragStart.cx) / rect.width) * 100;
            const pctY = ((e.clientY - dragStart.cy) / rect.height) * 100;
            props.onConnectorDrag(node.id, Math.max(3, Math.min(97, dragStart.ox + pctX)), Math.max(3, Math.min(97, dragStart.oy + pctY)));
          };
          const onPointerUp = (e: PointerEvent) => {
            (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
            if (!wasDragged) props.onSelectConnector(node.id);
            dragStart = null;
          };

          return (
            <div
              class={`colony-tree ${props.selection.id === node.id && props.selection.type === "connector" ? "is-selected" : ""}`}
              style={{
                left: `calc(${pos().x}% - ${size.width / 2}px)`,
                top: `calc(${pos().y}% - ${size.height}px)`,
                width: `${size.width}px`,
                height: `${size.height + 16}px`,
                cursor: "grab",
              }}
              data-connector-id={node.id}
              onPointerDown={onPointerDown}
              onPointerMove={onPointerMove}
              onPointerUp={onPointerUp}
              onClick={(e) => e.stopPropagation()}
            >
              <div class={`pixel-sprite ${spriteClass}`} />
              <div
                class="absolute"
                style={{
                  bottom: "-4px", left: "50%", transform: "translateX(-50%)",
                  width: "5px", height: "5px",
                  background: node.status === "active" ? "var(--color-status-ok)"
                    : node.status === "paused" ? "var(--color-status-warn)"
                    : "var(--color-status-idle)",
                }}
              />
              <div
                class="colony-text-3xs absolute whitespace-nowrap text-text-dim"
                style={{ bottom: "-18px", left: "50%", transform: "translateX(-50%)" }}
              >
                {node.label}
              </div>
            </div>
          );
        }}
      </For>

      {/* Output indicators near active nodes */}
      <For each={props.nodes.filter((n) => n.status === "active")}>
        {(node) => {
          const connectorPos = () => getConnectorPos(node.id);
          return (
            <For each={Array.from({ length: seeded(node.id + "mushcount", 1, 3) }, (_, i) => i)}>
              {(i) => {
                const spriteClass = MUSHROOM_SPRITES[seeded(node.id + "mt" + i, 0, 3)];
                const ox = seeded(node.id + "mx" + i, -4, 5);
                const oy = seeded(node.id + "my" + i, 5, 12);
                return (
                  <div
                    class="colony-mushroom"
                    style={{ left: `${connectorPos().x + ox}%`, top: `${connectorPos().y + oy}%` }}
                  >
                    <div class={`pixel-sprite ${spriteClass ?? "sprite-mushroom-gold"}`} />
                  </div>
                );
              }}
            </For>
          );
        }}
      </For>

      {/* Agents (porters) — with activity states + simlish */}
      <For each={props.agents}>
        {(agent) => {
          const pos = () => agentPos(agent);
          const spriteClass = ROLE_SPRITES[agent.role];
          const act = () => getAgentActivity(agent);
          const isSelected = () => props.selection.id === agent.id && props.selection.type === "agent";

          return (
            <div
              class={`colony-agent is-${act()} ${walkingAgents().has(agent.id) ? "is-walking" : ""} ${isSelected() ? "is-selected" : ""}`}
              style={{
                left: `calc(${pos().x}% - 14px)`,
                top: `calc(${pos().y}% - 10px)`,
              }}
              data-agent-id={agent.id}
            >
              {/* Simlish bubble — appears when activity state changes */}
              <Show when={bubbles()[agent.id]}>
                {(b) => (
                  <div class={`colony-bubble is-${b().type === "urgent" ? "urgent" : "normal"}`}>
                    {b().text}
                  </div>
                )}
              </Show>

              {/* Overhead info */}
              <div
                class="pointer-events-none absolute flex flex-col items-center gap-px"
                style={{ bottom: "100%", left: "50%", transform: "translateX(-50%)", "margin-bottom": "2px" }}
              >
                <span class="colony-text-sm" style={{ filter: "drop-shadow(0 0 2px var(--color-shadow-deep))" }}>
                  {act() === "error" ? "!!" : act() === "firing" ? "!" : act() === "idle" ? "-" : act() === "waiting" ? "~" : "*"}
                </span>
                {/* Fuel bars — visible when not at full */}
                <Show when={agent.fuel < 100 || agent.hp < 100}>
                  <div class="flex gap-px">
                    <div class="colony-fuel-bar">
                      <div
                        class={`colony-fuel-fill ${agent.fuelStatus === "ok" ? "bg-status-ok" : agent.fuelStatus === "warn" ? "bg-status-warn" : "bg-status-error"}`}
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
                <span class="colony-text-3xs text-text-dim">{agent.name}</span>
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
