import { createSignal, For, Show } from "solid-js";
import type { Component, JSX } from "solid-js";
import type { ColonyTree, ColonyAgent, ColonyConnection, ColonyFormation, ColonySelection } from "./types";
import type { EventItem } from "../dashboard/model";
import type { AvailableConnector, ConnectorSchema } from "@springtale/types";
import { ColonyCanvas } from "./ColonyCanvas";

export interface ViewportProps {
  trees: ColonyTree[];
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

  return (
    <div class="relative h-full w-full">
      {/* Colony Canvas */}
      <ColonyCanvas
        trees={props.trees}
        agents={props.agents}
        connections={props.connections}
        formations={props.formations}
        events={props.events}
        selection={props.selection}
        underground={underground()}
        connectorPositions={props.connectorPositions}
        onSelectConnector={props.onSelectConnector}
        onSelectAgent={props.onSelectAgent}
        onSelectFormation={props.onSelectFormation}
        onClearSelection={props.onClearSelection}
        onConnectorDrag={props.onConnectorDrag}
        onHatch={props.onHatch}
        availableConnectors={props.availableConnectors}
        connectorSchemas={props.connectorSchemas}
        onSetupConnector={props.onSetupConnector}
        onParseRule={props.onParseRule}
      />

      {/* Layer toggle — top left */}
      <div class="absolute left-1.5 top-1.5 z-[15] flex gap-0.5">
        <button
          class={`colony-layer-btn ${!underground() ? "is-active" : ""}`}
          onClick={() => setUnderground(false)}
        >
          SURFACE
        </button>
        <button
          class={`colony-layer-btn ${underground() ? "is-active" : ""}`}
          onClick={() => setUnderground(true)}
        >
          UNDERGROUND
        </button>
      </div>

      {/* Event feed — DRG Mission Control pattern, top right */}
      <div class="pointer-events-none absolute right-1.5 top-1.5 z-[15] w-[200px]">
        <For each={props.events.slice(0, 5)}>
          {(event) => (
            <div class={`colony-feed-entry ${event.severity === "error" ? "is-warning" : ""}`}>
              <span class="min-w-[32px] text-text-dim">
                {new Date(event.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
              </span>
              <span class={`min-w-[38px] font-bold ${event.severity === "error" ? "text-status-warn" : "text-status-ok"}`}>
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
                <span class="colony-text-2xs" style={{ color: "var(--color-status-ok)" }}>
                  {agent.pipeline ?? agent.name}
                </span>
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
