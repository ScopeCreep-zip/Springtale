import { For, Show, Switch, Match } from "solid-js";
import type { Component } from "solid-js";
import type {
  ColonyNode, ColonyAgent, ColonyConnection, ColonyFormation, ColonySelection, ColonyCommand, DetailView,
} from "./types";
import type { EventItem } from "../dashboard/model";
import { getAgentPosition, getFormationBounds, type ConnectorPositions } from "./geometry";
import {
  COMMANDS, ROLE_SPRITES, ROLE_COLORS,
  MOMENTUM_NAMES, MOMENTUM_COLORS, MOMENTUM_UNLOCKS, TIER_CAPABILITIES,
  NODE_SPRITES, seeded,
} from "./types";

export interface BottomPanelProps {
  nodes: ColonyNode[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  events: EventItem[];
  selection: ColonySelection;
  detailView: DetailView;
  onCommand: (action: string) => void;
  outputs?: Array<{ id: string; connector_name: string; rule_name: string | null; output_json: string; success: boolean; error_message: string | null; created_at: string }>;
  availableConnectors?: import("@springtale/types").AvailableConnector[];
  onSelectAgent?: (id: string) => void;
  onSelectConnector?: (id: string) => void;
  onSetupConnector?: (name: string) => void;
  onCreateBot?: () => void;
  onAddToFormation?: (formationId: string, connectorName: string) => Promise<void>;
  connectorPositions: ConnectorPositions;
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
        <div class="colony-label text-text-dim">
          NETWORK MAP
        </div>
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
            />
          </Match>
          <Match when={props.detailView.mode === "bots"}>
            <BotsListView agents={props.agents} onSelect={props.onSelectAgent} onCreateNew={props.onCreateBot} />
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
          <Match when={true}>
            <DetailPanel
              nodes={props.nodes}
              agents={props.agents}
              connections={props.connections}
              formations={props.formations}
              selection={props.selection}
            />
          </Match>
        </Switch>
      </div>

      {/* Zone 3: Command Grid */}
      <div class="border-l-2 border-bark p-1.5">
        <div class="colony-label text-text-dim">
          COMMANDS
        </div>
        <CommandGrid
          selection={props.selection}
          onCommand={props.onCommand}
        />
      </div>
    </>
  );
};

// ── Minimap ──────────────────────────────────────────────

const Minimap: Component<{
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
          const bounds = () => getFormationBounds(f, props.agents, props.nodes, props.connectorPositions);
          return (
            <div
              class="absolute rounded-[40%] opacity-30"
              style={{
                left: `${bounds().cx - bounds().rx}%`,
                top: `${bounds().cy - bounds().ry}%`,
                width: `${bounds().rx * 2}%`,
                height: `${bounds().ry * 2}%`,
                border: `1px solid ${f.color}`,
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
              style={{
                left: `${pos().x - 1}%`, top: `${pos().y - 1}%`,
                width: "4px", height: "4px",
                background: tree.status === "active" ? "var(--color-canopy)"
                  : tree.status === "paused" ? "var(--color-canopy-degraded)" : "var(--color-minimap-idle)",
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
                left: `${a().x}%`, top: `${a().y}%`,
                width: `${length()}%`,
                transform: `rotate(${angle()}deg)`, "transform-origin": "0 0",
                background: hasActive ? "var(--color-mycelium-active)" : "var(--color-mycelium)",
              }}
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
              style={{
                left: `${pos().x}%`, top: `${pos().y}%`,
                background: ROLE_COLORS[agent.role] ?? "var(--color-text-secondary)",
                ...(isSelected() ? { width: "5px", height: "5px" } : {}),
              }}
            />
          );
        }}
      </For>
    </div>
  );
};

// ── Detail Panel ─────────────────────────────────────────

const DetailPanel: Component<{
  nodes: ColonyNode[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  selection: ColonySelection;
}> = (props) => {
  return (
    <Switch fallback={
      <div>
        <div class="colony-label mb-1 text-text-dim">COLONY</div>
        <div class="colony-text-xs py-3 text-center text-text-dim">
          Click a node, agent, or formation
        </div>
      </div>
    }>
      {/* Agent detail */}
      <Match when={props.selection.type === "agent"}>
        {(() => {
          const agent = () => props.agents.find((a) => a.id === props.selection.id);
          const formation = () => {
            const a = agent();
            return a?.connectorId ? props.formations.find((f) => f.members.includes(a.connectorId!)) : undefined;
          };
          if (!agent()) return null;
          const a = agent()!;
          const fuelColor = a.fuelStatus === "ok" ? "var(--color-status-ok)" : a.fuelStatus === "warn" ? "var(--color-status-warn)" : "var(--color-status-error)";

          return (
            <div>
              <div class="colony-label mb-1 text-text-dim">AGENT</div>
              <div class="flex items-start gap-2">
                <div class="colony-portrait-frame">
                  <div class={`pixel-sprite ${ROLE_SPRITES[a.role]}`} style={{ transform: "scale(3)" }} />
                </div>
                <div class="min-w-0 flex-1">
                  <div class="colony-text-md font-bold">{a.name}</div>
                  <div class="colony-text-2xs uppercase text-text-dim" style={{ "letter-spacing": "1px" }}>
                    {a.role}{a.connectorId ? ` near ${a.connectorId}` : " roaming"}
                    {formation() ? ` / ${formation()!.name}` : ""}
                  </div>
                  <div class="colony-text-xs mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-status-warn">
                    {a.task}
                  </div>
                  {/* Stat bars */}
                  <div class="colony-stat-grid mt-1">
                    <span class="colony-text-3xs text-text-dim">FUEL</span>
                    <div class="colony-stat-bar">
                      <div class="colony-stat-fill" style={{ width: `${a.fuel}%`, background: fuelColor }} />
                    </div>
                    <span class="colony-text-5xs text-right" style={{ color: fuelColor }}>{a.fuel}</span>
                    <span class="colony-text-3xs text-text-dim">HP</span>
                    <div class="colony-stat-bar">
                      <div class="colony-stat-fill" style={{ width: `${a.hp}%`, background: "var(--color-role-scout)" }} />
                    </div>
                    <span class="colony-text-2xs text-right text-role-scout">{a.hp}</span>
                  </div>
                  <Show when={a.pipeline}>
                    <div class="colony-text-2xs mt-0.5 text-mycelium-active">
                      PIPELINE: {a.pipeline}
                    </div>
                  </Show>
                  {/* Autonomy pips */}
                  <div class="mt-1 flex gap-0.5">
                    <For each={[0, 1, 2, 3]}>
                      {(level) => (
                        <div
                          class={`colony-autonomy-pip ${a.autonomy === level ? "font-bold" : ""}`}
                          style={{
                            "border-color": a.autonomy === level
                              ? ["var(--color-status-ok)", "var(--color-role-scout)", "var(--color-status-warn)", "var(--color-status-error)", "var(--color-role-analyst)"][level]
                              : undefined,
                            background: a.autonomy === level
                              ? ["var(--color-status-ok)", "var(--color-role-scout)", "var(--color-status-warn)", "var(--color-status-error)", "var(--color-role-analyst)"][level]
                              : undefined,
                            color: a.autonomy === level ? "var(--color-soil-deep)" : undefined,
                          }}
                        >
                          {level}
                        </div>
                      )}
                    </For>
                  </div>
                  <div class="colony-text-3xs mt-0.5 text-text-dim">
                    {a.autonomyLabel}
                  </div>
                </div>
              </div>
            </div>
          );
        })()}
      </Match>

      {/* Tree detail */}
      <Match when={props.selection.type === "connector"}>
        {(() => {
          const tree = () => props.nodes.find((t) => t.id === props.selection.id);
          if (!tree()) return null;
          const t = tree()!;
          const relatedConns = () => props.connections.filter((c) => c.a === t.id || c.b === t.id);
          const nearbyAgents = () => props.agents.filter((a) => a.connectorId === t.id);
          const statusColor = t.status === "active" ? "var(--color-status-ok)" : t.status === "paused" ? "var(--color-status-warn)" : "var(--color-text-dim)";

          return (
            <div>
              <div class="colony-label mb-1 text-text-dim">CONNECTOR</div>
              <div class="colony-text-md font-bold">{t.label}</div>
              <div class="colony-text-5xs uppercase tracking-wider" style={{ color: statusColor }}>
                {t.status.toUpperCase()} {t.type.toUpperCase()}
              </div>
              <div class="colony-text-2xs mt-1.5 text-text-dim">CONNECTIONS</div>
              <For each={relatedConns()}>
                {(conn) => (
                  <For each={conn.pipes}>
                    {(pipe) => {
                      const pipeColor = pipe.status === "active" ? "var(--color-mycelium-active)" : "var(--color-text-dim)";
                      const direction = pipe.dir === 1 ? `${conn.a} > ${conn.b}` : `${conn.b} > ${conn.a}`;
                      return (
                        <div class="colony-text-2xs flex justify-between border-b border-bark py-0.5">
                          <span style={{ color: pipeColor }}>{pipe.id}</span>
                          <span>{direction}</span>
                        </div>
                      );
                    }}
                  </For>
                )}
              </For>
              <div class="colony-text-2xs mt-1.5 text-text-dim">NEARBY AGENTS</div>
              <For each={nearbyAgents()}>
                {(a) => (
                  <div class="colony-text-2xs flex justify-between border-b border-bark py-0.5">
                    <span>* {a.name}</span>
                    <span class="text-text-dim">{a.role}</span>
                  </div>
                )}
              </For>
            </div>
          );
        })()}
      </Match>

      {/* Formation detail */}
      <Match when={props.selection.type === "formation"}>
        {(() => {
          const formation = () => props.formations.find((f) => f.id === props.selection.id);
          if (!formation()) return null;
          const f = formation()!;
          const members = () => props.agents.filter((a) => a.connectorId && f.members.includes(a.connectorId));

          return (
            <div>
              <div class="colony-label mb-1 text-text-dim">FORMATION</div>
              <div class="font-bold" style={{ "font-size": "9px", color: f.color }}>{f.name}</div>
              <div class="colony-text-2xs text-text-dim">{f.intent}: {f.description}</div>
              {/* Momentum bar */}
              <div class="my-1.5 flex gap-0.5">
                <For each={MOMENTUM_NAMES}>
                  {(_, i) => (
                    <div
                      class="h-[5px] flex-1"
                      style={{
                        background: i() <= f.momentum ? MOMENTUM_COLORS[i()] : "var(--color-soil-darker)",
                      }}
                    />
                  )}
                </For>
              </div>
              <div class="colony-text-5xs mb-1" style={{ color: f.color }}>
                {f.momentumLabel} — {MOMENTUM_UNLOCKS[f.momentum]}
              </div>
              <For each={members()}>
                {(a) => {
                  const fuelColor = a.fuelStatus === "ok" ? "var(--color-status-ok)" : a.fuelStatus === "warn" ? "var(--color-status-warn)" : "var(--color-status-error)";
                  const healthIcon = a.status === "ok" ? "●" : a.status === "warn" ? "◐" : a.status === "error" ? "○" : "·";
                  const healthClass = a.status === "ok" ? "text-status-ok" : a.status === "warn" ? "text-status-warn" : a.status === "error" ? "text-status-error" : "text-text-dim";
                  return (
                    <div class="colony-text-2xs flex justify-between border-b border-bark py-0.5">
                      <span>
                        <span class={healthClass}>{healthIcon}</span>
                        {" "}{a.name} <span class="text-text-dim">{a.role}</span>
                      </span>
                      <span style={{ color: fuelColor }}>{a.fuel} fuel</span>
                    </div>
                  );
                }}
              </For>

              {/* Capability unlocks for current momentum tier */}
              <div class="colony-label mt-2 mb-1">CAPABILITIES ({f.momentumLabel})</div>
              <div class="flex flex-wrap gap-1">
                <For each={TIER_CAPABILITIES[f.momentum] ?? []}>
                  {(cap) => (
                    <span class="colony-text-5xs rounded border border-bark bg-soil-deep px-1.5 py-0.5 text-text-secondary">
                      {cap}
                    </span>
                  )}
                </For>
              </div>
            </div>
          );
        })()}
      </Match>
    </Switch>
  );
};

// ── Command Grid ─────────────────────────────────────────

const CommandGrid: Component<{
  selection: ColonySelection;
  onCommand: (label: string) => void;
}> = (props) => {
  const commands = () => COMMANDS[props.selection.type ?? "none"];

  return (
    <div class="grid h-[calc(100%-16px)] grid-cols-3 gap-0.5">
      <For each={commands()}>
        {(cmd) => {
          if (!cmd) return <div class="colony-command-btn is-empty" />;
          return (
            <button
              class="colony-command-btn"
              onClick={() => props.onCommand(cmd.action)}
            >
              <span class="colony-text-icon">{cmd.icon}</span>
              {cmd.label}
              <span class="colony-text-3xs bg-soil-deep px-0.5 text-text-dim">{cmd.key}</span>
            </button>
          );
        }}
      </For>
    </div>
  );
};

// ── List Views ───────────────────────────────────────────

const BotsListView: Component<{
  agents: ColonyAgent[];
  onSelect?: (id: string) => void;
  onCreateNew?: () => void;
}> = (props) => (
  <div>
    <div class="colony-label mb-1">BOTS ({props.agents.length})</div>
    <Show when={props.agents.length > 0 || props.onCreateNew} fallback={
      <p class="colony-text-xs py-2 text-text-dim">No bots yet.</p>
    }>
      <div class="colony-card-strip">
        <For each={props.agents}>
          {(agent) => {
            const statusClass = () =>
              agent.status === "ok" ? "is-active" : agent.status === "warn" ? "is-warn" : "";
            const roleColor = () => ROLE_COLORS[agent.role] ?? "var(--color-text-dim)";
            return (
              <button
                class={`colony-card ${statusClass()}`}
                onClick={() => props.onSelect?.(agent.id)}
              >
                <div class={`pixel-sprite ${ROLE_SPRITES[agent.role] ?? "sprite-worker"}`} style={{ transform: "scale(2)" }} />
                <div class="colony-text-3xs font-bold text-text-primary truncate w-full">{agent.name}</div>
                <div class="colony-text-5xs uppercase" style={{ color: roleColor() }}>{agent.role}</div>
                <div class="colony-text-5xs text-text-dim truncate w-full">{agent.connectorId ?? "roaming"}</div>
                {/* Fuel bar */}
                <div class="mt-auto w-full">
                  <div class="colony-stat-bar" style={{ height: "3px" }}>
                    <div class="colony-stat-fill" style={{
                      width: `${agent.fuel}%`,
                      background: agent.fuelStatus === "ok" ? "var(--color-status-ok)" : agent.fuelStatus === "warn" ? "var(--color-status-warn)" : "var(--color-status-error)",
                    }} />
                  </div>
                </div>
              </button>
            );
          }}
        </For>
        {/* + New Bot card */}
        <Show when={props.onCreateNew}>
          <button
            class="colony-card is-available"
            onClick={() => props.onCreateNew?.()}
          >
            <div class="colony-text-md text-status-ok">+</div>
            <div class="colony-text-3xs text-status-ok">New Bot</div>
          </button>
        </Show>
      </div>
    </Show>
  </div>
);

const ConnectorsListView: Component<{
  nodes: ColonyNode[];
  available: import("@springtale/types").AvailableConnector[];
  onSelect?: (id: string) => void;
  onSetup?: (name: string) => void;
}> = (props) => {
  const notLoaded = () => props.available.filter((a) => !a.loaded);

  return (
    <div>
      {/* Loaded connectors */}
      <div class="colony-label mb-1">LOADED ({props.nodes.length})</div>
      <Show when={props.nodes.length > 0} fallback={
        <p class="colony-text-2xs py-1 text-text-dim">No connectors loaded yet.</p>
      }>
        <div class="colony-card-strip mb-3">
          <For each={props.nodes}>
            {(tree) => {
              const spriteClass = NODE_SPRITES[tree.type] ?? "sprite-tree-deciduous";
              const statusClass = () =>
                tree.status === "active" ? "is-active" : tree.status === "paused" ? "is-warn" : "";
              return (
                <button
                  class={`colony-card ${statusClass()}`}
                  onClick={() => props.onSelect?.(tree.id)}
                >
                  <div class={`pixel-sprite ${spriteClass}`} style={{ transform: "scale(2)" }} />
                  <div class="colony-text-3xs font-bold text-text-primary truncate w-full">
                    {tree.label.replace("connector-", "")}
                  </div>
                  <div class={`colony-text-5xs uppercase ${
                    tree.status === "active" ? "text-status-ok" : tree.status === "paused" ? "text-status-warn" : "text-text-dim"
                  }`}>
                    {tree.status}
                  </div>
                </button>
              );
            }}
          </For>
        </div>
      </Show>

      {/* Available but not loaded */}
      <Show when={notLoaded().length > 0}>
        <div class="colony-label mb-1">AVAILABLE ({notLoaded().length})</div>
        <div class="colony-card-strip">
          <For each={notLoaded()}>
            {(connector) => {
              const treeType = ["conifer", "deciduous", "shrub"][seeded(connector.name + "type", 0, 3)] ?? "deciduous";
              const spriteClass = NODE_SPRITES[treeType as keyof typeof NODE_SPRITES] ?? "sprite-tree-deciduous";
              return (
                <button
                  class="colony-card is-available"
                  onClick={() => props.onSetup?.(connector.name)}
                >
                  <div class={`pixel-sprite ${spriteClass}`} style={{ transform: "scale(2)", opacity: 0.5 }} />
                  <div class="colony-text-3xs text-text-secondary truncate w-full">
                    {connector.name.replace("connector-", "")}
                  </div>
                  <div class="colony-text-5xs text-status-ok">
                    {connector.requires_config ? "Configure" : "Enable"}
                  </div>
                </button>
              );
            }}
          </For>
        </div>
      </Show>
    </div>
  );
};

const EventsListView: Component<{
  events: EventItem[];
  filterConnector?: string;
}> = (props) => {
  const filtered = () => {
    if (props.filterConnector) {
      return props.events.filter((e) => e.connectorName === props.filterConnector);
    }
    return props.events;
  };

  return (
    <div>
      <div class="colony-label mb-1">
        {props.filterConnector ? `EVENTS: ${props.filterConnector}` : "EVENTS"} ({filtered().length})
      </div>
      <Show when={filtered().length > 0} fallback={
        <p class="colony-text-xs py-2 text-text-dim">
          {props.filterConnector ? `No events for ${props.filterConnector}.` : "No events yet."}
        </p>
      }>
        <div class="space-y-0.5">
          <For each={filtered()}>
            {(event) => (
              <div class="flex items-center gap-2 border-b border-bark py-1">
                <span class="colony-text-3xs shrink-0 text-text-dim">
                  {new Date(event.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                </span>
                <span class={`colony-text-2xs shrink-0 font-bold ${
                  event.severity === "error" ? "text-status-warn" : "text-status-ok"
                }`}>
                  {event.connectorName}
                </span>
                <span class="colony-text-2xs truncate text-text-secondary">{event.triggerType}</span>
                <span class="colony-text-3xs ml-auto truncate text-text-dim">{event.actionTaken}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

const FormationsListView: Component<{
  formations: ColonyFormation[];
  agents: ColonyAgent[];
  addAgentId?: string;
  onAddToFormation?: (formationId: string, connectorName: string) => Promise<void>;
}> = (props) => (
  <div>
    <div class="colony-label mb-1">
      {props.addAgentId ? "SELECT FORMATION TO JOIN" : `FORMATIONS (${props.formations.length})`}
    </div>
    <Show when={props.formations.length > 0} fallback={
      <p class="colony-text-xs py-2 text-text-dim">No formations. Use NEW RULE to create one.</p>
    }>
      <div class="space-y-1">
        <For each={props.formations}>
          {(formation) => {
            const memberCount = formation.members.length;
            return (
              <button
                class="flex w-full items-center gap-2 rounded border border-bark p-2 text-start hover:border-bark-light"
                onClick={async () => {
                  if (props.addAgentId && props.onAddToFormation) {
                    const agent = props.agents.find((a) => a.id === props.addAgentId);
                    if (agent?.connectorId) {
                      await props.onAddToFormation(formation.id, agent.connectorId);
                    }
                  }
                }}
              >
                <span class="colony-text-xs font-bold" style={{ color: formation.color }}>{formation.name}</span>
                <span class="colony-text-3xs uppercase text-text-dim">{formation.intent}</span>
                <span class="colony-text-3xs ml-auto text-text-dim">{memberCount} members</span>
                <span class="colony-text-3xs font-bold" style={{ color: formation.color }}>{formation.momentumLabel}</span>
              </button>
            );
          }}
        </For>
      </div>
    </Show>
  </div>
);

const OutputsListView: Component<{
  outputs: Array<{ id: string; connector_name: string; rule_name: string | null; output_json: string; success: boolean; error_message: string | null; created_at: string }>;
  connectorId: string;
}> = (props) => (
  <div>
    <div class="colony-label mb-1">OUTPUTS: {props.connectorId} ({props.outputs.length})</div>
    <Show when={props.outputs.length > 0} fallback={
      <p class="colony-text-xs py-2 text-text-dim">No execution results yet for {props.connectorId}.</p>
    }>
      <div class="space-y-1">
        <For each={props.outputs}>
          {(output) => (
            <div class={`rounded border p-2 ${output.success ? "border-bark" : "border-status-error"}`}>
              <div class="flex items-center gap-2">
                <span class={`inline-block h-2 w-2 rounded-full ${output.success ? "bg-status-ok" : "bg-status-error"}`} />
                <span class="colony-text-2xs font-bold text-text-primary">{output.rule_name ?? "unknown"}</span>
                <span class="colony-text-3xs ml-auto text-text-dim">
                  {new Date(output.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                </span>
              </div>
              <Show when={output.error_message}>
                <p class="colony-text-3xs mt-1 text-status-error">{output.error_message}</p>
              </Show>
              <pre class="colony-text-3xs mt-1 max-h-20 overflow-auto whitespace-pre-wrap text-text-secondary">
                {(() => {
                  try { return JSON.stringify(JSON.parse(output.output_json), null, 2); }
                  catch { return output.output_json; }
                })()}
              </pre>
            </div>
          )}
        </For>
      </div>
    </Show>
  </div>
);
