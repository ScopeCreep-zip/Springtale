import type { AvailableConnector, ConnectorSchema } from "@springtale/types";
import type { Component, JSX } from "solid-js";
import { createSignal, For, Show } from "solid-js";
import { activityOf } from "../dashboard/activity";
import type { EventItem } from "../dashboard/model";
import type { Utterance } from "../dashboard/types";
import type { Locale } from "../i18n/types";
import { ColonyCanvas } from "./ColonyCanvas";
import { EventRibbon } from "./EventRibbon";
import { getAgentPosition, getConnectorPosition, getFormationBounds } from "./geometry";
import type { OverlayMode } from "./overlay";
import { overlayLabel } from "./overlay";
import type {
  ColonyAgent,
  ColonyConnection,
  ColonyFormation,
  ColonyNode,
  ColonySelection,
} from "./types";

/** Wheel-zoom bounds. 1 is "the canvas fills the viewport". */
const ZOOM_MIN = 0.5;
const ZOOM_MAX = 3;
const ZOOM_STEP = 1.1;

export interface ViewportProps {
  nodes: ColonyNode[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  events: EventItem[];
  selection: ColonySelection;
  connectorPositions: Record<string, { x: number; y: number }>;
  onSelectConnector: (id: string) => void;
  onSelectAgent: (id: string) => void;
  onSelectFormation: (id: string) => void;
  onClearSelection: () => void;
  onConnectorDrag: (id: string, x: number, y: number) => void;
  onHatch?: () => void;
  // Plan 3.4 — motes. Locale and text direction come from useI18n() inside.
  utterances: Utterance[];
  colonyNow: number;
  agentToConnector: Record<string, string>;
  framesFor: (u: Utterance, locale: Locale) => string[];
  roleOf: (agentId: string) => string | undefined;
  viewScale?: number; // 3.5 supplies it; 1 until then
  /** Plan 3.6 — which field recolours the springtails. */
  canvasOverlay?: OverlayMode;
  overlay?: JSX.Element;
  // OOBE TeamBuilder props
  availableConnectors?: AvailableConnector[];
  connectorSchemas?: ConnectorSchema[];
  onSetupConnector?: (name: string) => void;
  onParseRule?: (intent: string) => Promise<Record<string, unknown>>;
}

/**
 * Viewport — wraps ColonyCanvas with overlays.
 *
 * Contains: layer toggle, event feed, pipeline tickets,
 * plus the main canvas underneath.
 */
export const Viewport: Component<ViewportProps> = (props) => {
  const [underground, setUnderground] = createSignal(false);

  // ── Pan + zoom (plan 3.5) ──────────────────────────────
  // A transform wrapper, not a scroll container: the canvas lays out in
  // percentages, so scaling it keeps every position calculation untouched
  // (pointer maths reads the *scaled* bounding rect, which stays correct).
  // Wheel zooms about the cursor; middle-drag pans, the CAD/RTS convention
  // that leaves left-drag free for moving trees.
  const [zoom, setZoom] = createSignal(1);
  const [pan, setPan] = createSignal({ x: 0, y: 0 });
  let shell: HTMLDivElement | undefined;
  let panStart: { cx: number; cy: number; px: number; py: number } | null = null;

  const onWheel = (e: WheelEvent) => {
    const rect = shell?.getBoundingClientRect();
    if (!rect) return;
    e.preventDefault();
    const before = zoom();
    const after = Math.min(
      ZOOM_MAX,
      Math.max(ZOOM_MIN, e.deltaY < 0 ? before * ZOOM_STEP : before / ZOOM_STEP),
    );
    if (after === before) return;
    // Keep the point under the cursor pinned.
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    const p = pan();
    setPan({
      x: cx - ((cx - p.x) * after) / before,
      y: cy - ((cy - p.y) * after) / before,
    });
    setZoom(after);
  };

  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 1) return; // middle button only
    e.preventDefault();
    const p = pan();
    panStart = { cx: e.clientX, cy: e.clientY, px: p.x, py: p.y };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: PointerEvent) => {
    if (!panStart) return;
    setPan({
      x: panStart.px + (e.clientX - panStart.cx),
      y: panStart.py + (e.clientY - panStart.cy),
    });
  };
  const onPointerUp = (e: PointerEvent) => {
    if (!panStart) return;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    panStart = null;
  };

  /** Scroll the viewport so a canvas-percentage point sits in the middle. */
  const centerOn = (pct: { x: number; y: number }) => {
    const rect = shell?.getBoundingClientRect();
    if (!rect) return;
    const z = zoom();
    setPan({
      x: rect.width / 2 - (pct.x / 100) * rect.width * z,
      y: rect.height / 2 - (pct.y / 100) * rect.height * z,
    });
  };

  /** The alert stack's jump: select the subject, then scroll to it. */
  const jumpTo = (sel: ColonySelection) => {
    if (!sel.id || !sel.type) return;
    const act = (a: ColonyAgent) =>
      activityOf(a, props.utterances, props.colonyNow, props.agentToConnector);
    if (sel.type === "connector") {
      props.onSelectConnector(sel.id);
      centerOn(getConnectorPosition(sel.id, props.nodes, props.connectorPositions));
      return;
    }
    if (sel.type === "agent") {
      const agent = props.agents.find((a) => a.id === sel.id);
      props.onSelectAgent(sel.id);
      if (agent)
        centerOn(getAgentPosition(agent, props.nodes, props.connectorPositions, act(agent)));
      return;
    }
    const formation = props.formations.find((f) => f.id === sel.id);
    props.onSelectFormation(sel.id);
    if (formation) {
      const b = getFormationBounds(
        formation,
        props.agents,
        props.nodes,
        props.connectorPositions,
        act,
      );
      centerOn({ x: b.cx, y: b.cy });
    }
  };

  return (
    <div
      ref={(el) => {
        shell = el;
      }}
      class="colony-viewport relative h-full w-full overflow-hidden"
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
    >
      {/* Pan/zoom transform wrapper — transform-origin 0 0 lives in colony.css */}
      <div
        class="colony-viewport-transform absolute inset-0"
        style={{ transform: `translate(${pan().x}px, ${pan().y}px) scale(${zoom()})` }}
      >
        {/* Colony Canvas */}
        <ColonyCanvas
          nodes={props.nodes}
          agents={props.agents}
          connections={props.connections}
          formations={props.formations}
          events={props.events}
          selection={props.selection}
          underground={underground()}
          connectorPositions={props.connectorPositions}
          onSelectConnector={props.onSelectConnector}
          onSelectAgent={props.onSelectAgent}
          utterances={props.utterances}
          colonyNow={props.colonyNow}
          agentToConnector={props.agentToConnector}
          framesFor={props.framesFor}
          roleOf={props.roleOf}
          viewScale={(props.viewScale ?? 1) * zoom()}
          overlay={props.canvasOverlay}
          onSelectFormation={props.onSelectFormation}
          onClearSelection={props.onClearSelection}
          onConnectorDrag={props.onConnectorDrag}
          onHatch={props.onHatch}
          availableConnectors={props.availableConnectors}
          connectorSchemas={props.connectorSchemas}
          onSetupConnector={props.onSetupConnector}
          onParseRule={props.onParseRule}
        />
      </div>

      {/* Plan 3.6 — the alert stack, one entry per live condition. */}
      <EventRibbon onJump={jumpTo} />

      {/* Overlay chip — names the field currently recolouring the canvas. */}
      <Show when={props.canvasOverlay && props.canvasOverlay !== "none"}>
        <div class="colony-overlay-chip absolute left-1.5 top-8 z-[15]">
          {overlayLabel(props.canvasOverlay ?? "none")} OVERLAY
        </div>
      </Show>

      {/* Layer toggle — top left */}
      <div class="absolute left-1.5 top-1.5 z-[15] flex gap-0.5">
        <button
          type="button"
          class={`colony-layer-btn ${!underground() ? "is-active" : ""}`}
          onClick={() => setUnderground(false)}
        >
          NETWORK
        </button>
        <button
          type="button"
          class={`colony-layer-btn ${underground() ? "is-active" : ""}`}
          onClick={() => setUnderground(true)}
        >
          DEPTH
        </button>
      </div>

      {/* Event feed — DRG Mission Control pattern, top right */}
      <div class="pointer-events-none absolute right-1.5 top-1.5 z-[15] w-[200px]">
        <For each={props.events.slice(0, 5)}>
          {(event) => (
            <div class={`colony-feed-entry ${event.severity === "error" ? "is-warning" : ""}`}>
              <span class="min-w-[32px] text-text-dim">
                {new Date(event.timestamp).toLocaleTimeString([], {
                  hour: "2-digit",
                  minute: "2-digit",
                })}
              </span>
              <span
                class={`min-w-[38px] font-bold ${event.severity === "error" ? "text-status-warn" : "text-status-ok"}`}
              >
                {event.connectorName}
              </span>
              <span class="overflow-hidden text-ellipsis whitespace-nowrap text-text-secondary">
                {event.actionTaken}
              </span>
            </div>
          )}
        </For>
      </div>

      {/* Pipeline tickets — Overcooked-style active work indicators */}
      <Show when={props.agents.some((a) => a.status === "ok")}>
        <div class="pointer-events-none absolute bottom-2 left-1/2 z-[15] flex -translate-x-1/2 gap-1.5">
          <For each={props.agents.filter((a) => a.status === "ok").slice(0, 4)}>
            {(agent) => (
              <div class="colony-ticket">
                <span class="colony-text-2xs text-status-ok">{agent.pipeline ?? agent.name}</span>
                <div class="colony-ticket-timer">
                  <div class="h-full bg-status-ok" style={{ width: `${agent.fuel}%` }} />
                </div>
              </div>
            )}
          </For>
        </div>
      </Show>

      {/* Overlay content (vault, hatch wizard, settings, etc.) */}
      <Show when={props.overlay}>
        <div class="absolute inset-0 z-[25] flex items-center justify-center bg-soil-deep/80">
          {props.overlay}
        </div>
      </Show>
    </div>
  );
};
