import type { Component } from "solid-js";
import { Match, Switch } from "solid-js";
import type { EventItem } from "../../dashboard/model";
import type { CommandDecl, ConnectorOutput } from "../../dashboard/types";
import type { ConnectorPositions } from "../geometry";
import type {
  ColonyAgent,
  ColonyConnection,
  ColonyFormation,
  ColonyNode,
  ColonySelection,
  DetailView,
} from "../types";
import { BotsListView } from "./BotsListView";
import { CanvasOutputView } from "./CanvasOutputView";
import { CommandGrid } from "./CommandGrid";
import { ConnectorsListView } from "./ConnectorsListView";
import { DetailPanel } from "./DetailPanel";
import { EventsListView } from "./EventsListView";
import { FormationsListView } from "./FormationsListView";
import { Minimap } from "./Minimap";
import { OutputsListView } from "./OutputsListView";

export interface BottomPanelProps {
  nodes: ColonyNode[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  events: EventItem[];
  selection: ColonySelection;
  detailView: DetailView;
  onCommand: (action: string) => void;
  outputs?: ConnectorOutput[];
  availableConnectors?: import("@springtale/types").AvailableConnector[];
  onSelectAgent?: (id: string) => void;
  onSelectConnector?: (id: string) => void;
  onSetupConnector?: (name: string) => void;
  onCreateBot?: () => void;
  onAddToFormation?: (formationId: string, connectorName: string) => Promise<void>;
  connectorPositions: ConnectorPositions;
  /**
   * F1 + B11: backend-supplied formation command list — when present,
   * `CommandGrid` renders this instead of the static `COMMANDS.formation`
   * table. Each entry includes status-aware enabled/disabled state +
   * canonical hotkey decided server-side.
   */
  formationCommands?: CommandDecl[];
}

/**
 * Bottom Panel — StarCraft 3-zone layout.
 *
 * Zone 1: Minimap (140px) — simplified canvas representation
 * Zone 2: Detail Panel (1fr) — selected entity info
 * Zone 3: Command Grid (170px) — 3x3 action buttons with hotkeys
 */
export const BottomPanel: Component<BottomPanelProps> = (props) => {
  return (
    <>
      {/* Zone 1: Minimap */}
      <div class="border-r-2 border-bark p-1.5">
        <div class="colony-label text-text-dim">NETWORK MAP</div>
        <Minimap
          nodes={props.nodes}
          agents={props.agents}
          connections={props.connections}
          formations={props.formations}
          selection={props.selection}
          connectorPositions={props.connectorPositions}
        />
      </div>

      {/* Zone 2: Detail Panel — view-mode driven */}
      <div class="colony-scrollbar overflow-y-auto p-1.5 px-2.5">
        <Switch>
          <Match when={props.detailView.mode === "entity" && props.selection.type}>
            <DetailPanel
              nodes={props.nodes}
              agents={props.agents}
              connections={props.connections}
              formations={props.formations}
              selection={props.selection}
              onCommand={props.onCommand}
            />
          </Match>
          <Match when={props.detailView.mode === "bots"}>
            <BotsListView
              agents={props.agents}
              onSelect={props.onSelectAgent}
              onCreateNew={props.onCreateBot}
            />
          </Match>
          <Match when={props.detailView.mode === "connectors"}>
            <ConnectorsListView
              nodes={props.nodes}
              available={props.availableConnectors ?? []}
              onSelect={props.onSelectConnector}
              onSetup={props.onSetupConnector}
            />
          </Match>
          <Match when={props.detailView.mode === "events"}>
            <EventsListView
              events={props.events}
              filterConnector={(props.detailView as { filterConnector?: string }).filterConnector}
            />
          </Match>
          <Match when={props.detailView.mode === "outputs"}>
            <OutputsListView
              outputs={props.outputs ?? []}
              connectorId={(props.detailView as { connectorId?: string }).connectorId ?? ""}
            />
          </Match>
          <Match when={props.detailView.mode === "formations"}>
            <FormationsListView
              formations={props.formations}
              agents={props.agents}
              addAgentId={(props.detailView as { addAgentId?: string }).addAgentId}
              onAddToFormation={props.onAddToFormation}
            />
          </Match>
          <Match when={props.detailView.mode === "canvas"}>
            <CanvasOutputView />
          </Match>
          <Match when={true}>
            <DetailPanel
              nodes={props.nodes}
              agents={props.agents}
              connections={props.connections}
              formations={props.formations}
              selection={props.selection}
              onCommand={props.onCommand}
            />
          </Match>
        </Switch>
      </div>

      {/* Zone 3: Command Grid */}
      <div class="border-l-2 border-bark p-1.5">
        <div class="colony-label text-text-dim">COMMANDS</div>
        <CommandGrid
          selection={props.selection}
          onCommand={props.onCommand}
          formationCommands={props.formationCommands}
        />
      </div>
    </>
  );
};

// ── Minimap ──────────────────────────────────────────────
