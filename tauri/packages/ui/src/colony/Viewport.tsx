import { createSignal, For, Show } from "solid-js";
import type { Component, JSX } from "solid-js";
import type { ColonyTree, ColonyAgent, ColonyConnection, ColonyFormation, ColonySelection } from "./types";
import type { EventItem } from "../CommandPanel";
import { ColonyCanvas } from "./ColonyCanvas";

export interface ViewportProps {
  trees: ColonyTree[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  events: EventItem[];
  selection: ColonySelection;
  onSelectTree: (id: string) => void;
  onSelectAgent: (id: string) => void;
  onSelectFormation: (id: string) => void;
  onClearSelection: () => void;
  /** Optional overlay content (vault dialog, hatch wizard, etc.) */
  overlay?: JSX.Element;
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
        selection={props.selection}
        underground={underground()}
        onSelectTree={props.onSelectTree}
        onSelectAgent={props.onSelectAgent}
        onSelectFormation={props.onSelectFormation}
        onClearSelection={props.onClearSelection}
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
            <div class={`colony-feed-entry ${event.actionTaken.includes("BLOCK") || event.actionTaken.includes("fail") ? "is-warning" : ""}`}>
              <span class="min-w-[32px] text-text-dim">
                {new Date(event.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
              </span>
              <span class={`min-w-[38px] font-bold ${event.actionTaken.includes("BLOCK") || event.actionTaken.includes("fail") ? "text-status-warn" : "text-status-ok"}`}>
                {event.connectorName}
              </span>
              <span class="overflow-hidden text-ellipsis whitespace-nowrap text-text-secondary">
                {event.actionTaken}
              </span>
            </div>
          )}
        </For>
      </div>

      {/* Overlay content (vault, hatch wizard, settings, etc.) */}
      <Show when={props.overlay}>
        <div class="absolute inset-0 z-[25] flex items-center justify-center bg-soil-deep/80">
          {props.overlay}
        </div>
      </Show>
    </div>
  );
};
