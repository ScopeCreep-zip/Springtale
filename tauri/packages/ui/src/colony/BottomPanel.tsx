import { For, Show, Switch, Match, createResource } from "solid-js";
import type { Component } from "solid-js";
import type {
  ColonyNode, ColonyAgent, ColonyConnection, ColonyFormation, ColonySelection, ColonyCommand, DetailView,
} from "./types";
import type { CommandDecl, CooperationEvent, CooperationEventEnvelope } from "../dashboard/types";
import type { EventItem } from "../dashboard/model";
import { useDashboard } from "../dashboard/context";
import { getAgentPosition, getFormationBounds, type ConnectorPositions } from "./geometry";
import {
  COMMANDS, ROLE_SPRITES, ROLE_COLORS,
  MOMENTUM_NAMES, MOMENTUM_COLORS, MOMENTUM_UNLOCKS, TIER_CAPABILITIES,
  NODE_SPRITES, NODE_SIZES, seeded,
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
              onCommand={props.onCommand}
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
              onCommand={props.onCommand}
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
          formationCommands={props.formationCommands}
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
  /** Command dispatcher — forwarded from BottomPanel so the formation
   *  detail card can surface clickable rows (e.g. G7 AI override) that
   *  open modals owned by the App. */
  onCommand: (action: string) => void;
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
                  {/* Health badge */}
                  <div class="colony-text-5xs mt-0.5 uppercase tracking-wider" style={{
                    color: a.healthState === "healthy" ? "var(--color-status-ok)"
                      : a.healthState === "degraded" ? "var(--color-status-warn)"
                      : "var(--color-status-error)",
                  }}>
                    {a.healthState ?? "healthy"} {(a.liveness ?? 1) < 0.5 ? `| SUSPECT` : ""}
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
                      <div class="colony-stat-fill" style={{
                        width: `${a.hp}%`,
                        background: a.healthState === "degraded" ? "var(--color-status-warn)"
                          : a.healthState === "critical" ? "var(--color-status-error)"
                          : "var(--color-role-scout)",
                      }} />
                    </div>
                    <span class="colony-text-2xs text-right" style={{
                      color: a.healthState === "degraded" ? "var(--color-status-warn)"
                        : a.healthState === "critical" ? "var(--color-status-error)"
                        : "var(--color-role-scout)",
                    }}>{a.hp}</span>
                    <span class="colony-text-3xs text-text-dim">LOAD</span>
                    <div class="colony-stat-bar">
                      <div class="colony-stat-fill" style={{
                        width: `${Math.round((a.attentionLoad ?? 0) * 100)}%`,
                        background: (a.attentionLoad ?? 0) > 0.8 ? "var(--color-status-error)"
                          : (a.attentionLoad ?? 0) > 0.5 ? "var(--color-status-warn)"
                          : "var(--color-status-ok)",
                      }} />
                    </div>
                    <span class="colony-text-5xs text-right text-text-dim">{Math.round((a.attentionLoad ?? 0) * 100)}%</span>
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
              <div class="flex items-center gap-2">
                <div class="font-bold" style={{ "font-size": "9px", color: f.color }}>{f.name}</div>
                <Show when={f.status === "paused"}>
                  <span class="colony-text-5xs rounded bg-status-warn px-1 text-soil-deep">PAUSED</span>
                </Show>
                <Show when={f.status === "draft"}>
                  <span class="colony-text-5xs rounded border border-bark px-1 text-text-dim">DRAFT</span>
                </Show>
                <Show when={f.guardStatus === "OK"}>
                  <span class="colony-text-5xs rounded bg-status-ok px-1 text-soil-deep">GUARD</span>
                </Show>
              </div>
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

              {/* Rally pips (Monster Hunter carts) */}
              <div class="mb-1 flex items-center gap-1">
                <span class="colony-text-3xs text-text-dim">RALLY</span>
                <div class="flex gap-0.5">
                  <For each={Array.from({ length: f.rallyMax })}>
                    {(_, i) => (
                      <div
                        class="h-[6px] w-[6px] rounded-sm"
                        style={{
                          background: i() < f.rallyTokens ? "var(--color-status-warn)" : "var(--color-soil-darker)",
                          border: "1px solid var(--color-bark)",
                        }}
                      />
                    )}
                  </For>
                </div>
                <span class="colony-text-5xs text-text-dim">{f.rallyTokens}/{f.rallyMax}</span>
              </div>

              {/* Aggregate stats row */}
              {(() => {
                const operational = members().filter((a) => a.healthState === "healthy").length;
                const avgLoad = members().length > 0
                  ? Math.round(members().reduce((sum, a) => sum + (a.attentionLoad ?? 0), 0) / members().length * 100)
                  : 0;
                const totalFuel = members().reduce((sum, a) => sum + a.fuel, 0);
                return (
                  <div class="colony-text-5xs mb-1 flex gap-2 text-text-dim">
                    <span>{operational}/{members().length} OPS</span>
                    <span>LOAD {avgLoad}%</span>
                    <span>FUEL {totalFuel}</span>
                  </div>
                );
              })()}

              {/* Member traffic-light grid (SC2 wireframe pattern) */}
              <div class="colony-label mb-0.5">MEMBERS</div>
              <For each={members()}>
                {(a) => {
                  const healthColor = a.healthState === "healthy" ? "var(--color-status-ok)"
                    : a.healthState === "degraded" ? "var(--color-status-warn)"
                    : a.healthState === "critical" ? "var(--color-status-error)"
                    : "var(--color-text-dim)";
                  const livenessIcon = (a.liveness ?? 1) > 0.8 ? "●"
                    : (a.liveness ?? 1) > 0.3 ? "◐"
                    : "○";
                  const fuelColor = a.fuelStatus === "ok" ? "var(--color-status-ok)" : a.fuelStatus === "warn" ? "var(--color-status-warn)" : "var(--color-status-error)";
                  return (
                    <div class="colony-text-2xs flex justify-between border-b border-bark py-0.5">
                      <span>
                        <span style={{ color: healthColor }}>{livenessIcon}</span>
                        {" "}{a.name} <span class="text-text-dim">{a.role}</span>
                      </span>
                      <span class="flex gap-1.5">
                        <Show when={(a.attentionLoad ?? 0) > 0.5}>
                          <span class="text-status-warn">{Math.round((a.attentionLoad ?? 0) * 100)}%</span>
                        </Show>
                        <span style={{ color: fuelColor }}>{a.fuel}</span>
                      </span>
                    </div>
                  );
                }}
              </For>

              {/* Attention distribution bar (Army of Two aggro meter) */}
              <Show when={members().length > 1}>
                <div class="colony-label mt-1.5 mb-0.5">ATTENTION</div>
                <div class="flex h-[6px] w-full overflow-hidden rounded-sm border border-bark">
                  <For each={members()}>
                    {(a) => {
                      const share = (a.attentionLoad ?? 0) * 100;
                      return (
                        <div
                          style={{
                            width: `${Math.max(share, 5)}%`,
                            background: ROLE_COLORS[a.role] ?? "var(--color-text-dim)",
                          }}
                          title={`${a.name}: ${Math.round(share)}%`}
                        />
                      );
                    }}
                  </For>
                </div>
              </Show>

              {/* Capability unlocks for current momentum tier */}
              <div class="colony-label mt-1.5 mb-0.5">CAPABILITIES ({f.momentumLabel})</div>
              <div class="flex flex-wrap gap-1">
                <For each={TIER_CAPABILITIES[f.momentum] ?? []}>
                  {(cap) => (
                    <span class="colony-text-5xs rounded border border-bark bg-soil-deep px-1.5 py-0.5 text-text-secondary">
                      {cap}
                    </span>
                  )}
                </For>
              </div>

              {/* G7 — per-formation AI adapter override row */}
              <FormationAiAdapterRow formationId={f.id} onCommand={props.onCommand} />

              {/* Phase H6 — per-formation cooperation event log */}
              <FormationEventLog formationId={f.id} />
            </div>
          );
        })()}
      </Match>
    </Switch>
  );
};

// ── Formation event log (Phase H6) ───────────────────────

const EVENT_LABELS: Record<CooperationEvent["kind"], string> = {
  intervention_fired:    "INTERVENTION",
  pacing_phase_changed:  "PACING",
  cascade_hit:           "CASCADE",
  consensus_vote_opened: "VOTE OPEN",
  consensus_vote_resolved: "VOTE END",
  commit_phase_changed:  "COMMIT",
  sacrifice_yield:       "YIELD",
  role_transformed:      "ROLE",
  member_marked_down:    "DOWN",
  supervisor_escalated:  "ESCALATE",
  recovery_action_taken: "RECOVERY",
  surface_deposited:     "SURFACE",
  interference_detected: "INTERFERE",
  cfp_round_started:     "CFP OPEN",
  cfp_round_resolved:    "CFP END",
  cbba_replan_requested: "REPLAN",
  cbba_replan_resolved:  "REPLAN END",
};

function severityFor(kind: CooperationEvent["kind"]): "error" | "warn" | "ok" | "idle" {
  switch (kind) {
    case "intervention_fired":
    case "supervisor_escalated":
    case "cascade_hit":
    case "interference_detected":
      return "error";
    case "member_marked_down":
    case "pacing_phase_changed":
    case "consensus_vote_opened":
    case "cfp_round_started":
    case "cbba_replan_requested":
      return "warn";
    case "sacrifice_yield":
    case "recovery_action_taken":
    case "consensus_vote_resolved":
    case "cfp_round_resolved":
    case "cbba_replan_resolved":
    case "commit_phase_changed":
    case "role_transformed":
    case "surface_deposited":
      return "ok";
    default:
      return "idle";
  }
}

function detailFor(event: CooperationEvent): string {
  switch (event.kind) {
    case "intervention_fired":     return event.summary;
    case "pacing_phase_changed":   return `${event.from} → ${event.to}`;
    case "cascade_hit":            return `streak ${event.streak} | ${event.members_affected} affected`;
    case "consensus_vote_opened":  return `${event.vote_id.slice(0, 8)} ${event.deadline_ms}ms`;
    case "consensus_vote_resolved": return `${event.vote_id.slice(0, 8)} → ${event.outcome}`;
    case "commit_phase_changed":   return `${event.barrier_id.slice(0, 8)} → ${event.phase}`;
    case "sacrifice_yield":        return `${event.sacrificer.slice(0, 8)} → ${event.beneficiary.slice(0, 8)} (${event.utility.toFixed(2)})`;
    case "role_transformed":       return `${event.agent.slice(0, 8)} ${event.from} → ${event.to}`;
    case "member_marked_down":     return `${event.agent.slice(0, 8)} tick ${event.since_tick}`;
    case "supervisor_escalated":   return event.reason;
    case "recovery_action_taken":  return `${event.helper.slice(0, 8)} → ${event.in_distress.slice(0, 8)} (${event.action})`;
    case "surface_deposited":      return `${event.agent.slice(0, 8)} ${event.surface_kind} ${event.ttl_ms}ms`;
    case "interference_detected":  return `${event.interference_kind} (${event.agents.length})`;
    case "cfp_round_started":      return `${event.cfp_id.slice(0, 8)} ${event.capability}`;
    case "cfp_round_resolved":     return `${event.cfp_id.slice(0, 8)} → ${event.winner?.slice(0, 8) ?? "no winner"}`;
    case "cbba_replan_requested":  return event.reason;
    case "cbba_replan_resolved":   return `${event.outcome.status} ${event.outcome.sweeps}s ${event.outcome.assigned}/${event.outcome.assigned + event.outcome.unassigned}`;
  }
}

const FormationEventLog: Component<{ formationId: string }> = (props) => {
  const db = useDashboard();
  const filtered = (): CooperationEventEnvelope[] =>
    db.cooperationEvents()
      .filter((env) => "formation_id" in env.event && env.event.formation_id === props.formationId)
      .slice(0, 50);

  return (
    <>
      <div class="colony-label mt-1.5 mb-0.5">EVENTS ({filtered().length})</div>
      <Show
        when={filtered().length > 0}
        fallback={
          <p class="colony-text-3xs py-0.5 text-text-dim">No cooperation events yet.</p>
        }
      >
        <div class="colony-event-log">
          <For each={filtered()}>
            {(env) => (
              <div class="colony-event-log-entry" data-severity={severityFor(env.event.kind)}>
                <span class="colony-event-log-time">
                  {new Date(env.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                </span>
                <span class="colony-event-log-kind">{EVENT_LABELS[env.event.kind]}</span>
                <span class="colony-event-log-detail">{detailFor(env.event)}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </>
  );
};

// ── Formation AI adapter row (G7) ────────────────────────

/**
 * Renders the currently-bound AI adapter for this formation (read from
 * `ai:formation:{id}` config key). Clicking dispatches
 * `formation:ai_adapter` so the host App can open the shared
 * `AiConfigPanel` scoped to the formation. Adapter-resolution precedence
 * (agent → formation → global) lives in `resolve_ai_config` server-side
 * per the thin-frontend rule.
 */
const FormationAiAdapterRow: Component<{
  formationId: string;
  onCommand: (action: string) => void;
}> = (props) => {
  const db = useDashboard();
  const [config] = createResource(
    () => props.formationId,
    async (id) => {
      try {
        return await db.provider.getConfig(`ai:formation:${id}`);
      } catch {
        return null;
      }
    },
  );

  const label = () => {
    const c = config();
    if (!c || (typeof c === "object" && c !== null && Object.keys(c).length === 0)) {
      return "inherit";
    }
    const t = (c as { type?: string }).type;
    return t ? t.toUpperCase() : "inherit";
  };

  return (
    <>
      <div class="colony-label mt-1.5 mb-0.5">AI ADAPTER</div>
      <button
        class="colony-text-2xs flex w-full items-center justify-between rounded border border-bark bg-soil-deep px-2 py-1 hover:border-bark-light"
        onClick={() => props.onCommand("formation:ai_adapter")}
      >
        <span class="text-text-secondary">{label()}</span>
        <span class="colony-text-5xs text-text-dim">click to override</span>
      </button>
    </>
  );
};

// ── Command Grid ─────────────────────────────────────────

const CommandGrid: Component<{
  selection: ColonySelection;
  onCommand: (label: string) => void;
  formationCommands?: CommandDecl[];
}> = (props) => {
  // F1: formation context renders ONLY backend-supplied commands (B11);
  // there is no static fallback. Other selection contexts use the
  // hardcoded `COMMANDS` table for now (those grids are still
  // frontend-owned per colony-canvas.md §3 — only formation commands
  // need backend status-awareness).
  return (
    <Show
      when={props.selection.type !== "formation"}
      fallback={
        <div class="grid h-[calc(100%-16px)] grid-cols-3 gap-0.5">
          <For each={props.formationCommands ?? []}>
            {(cmd) => (
              <button
                class="colony-command-btn"
                classList={{ "is-disabled": !cmd.enabled }}
                disabled={!cmd.enabled}
                title={cmd.disabled_reason ?? cmd.label}
                onClick={() => props.onCommand(cmd.id)}
              >
                <span class="colony-text-icon">{cmd.icon}</span>
                {cmd.label}
                <span class="colony-text-3xs bg-soil-deep px-0.5 text-text-dim">
                  {cmd.hotkey}
                </span>
              </button>
            )}
          </For>
        </div>
      }
    >
      <div class="grid h-[calc(100%-16px)] grid-cols-3 gap-0.5">
        <For each={COMMANDS[props.selection.type ?? "none"]}>
          {(cmd) => {
            if (!cmd) return <div class="colony-command-btn is-empty" />;
            return (
              <button
                class="colony-command-btn"
                onClick={() => props.onCommand(cmd.action)}
              >
                <span class="colony-text-icon">{cmd.icon}</span>
                {cmd.label}
                <span class="colony-text-3xs bg-soil-deep px-0.5 text-text-dim">
                  {cmd.key}
                </span>
              </button>
            );
          }}
        </For>
      </div>
    </Show>
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
            {(node) => {
              const spriteClass = NODE_SPRITES[node.type] ?? "sprite-tree-deciduous";
              const size = NODE_SIZES[node.type] ?? { width: 36, height: 44 };
              const statusClass = () =>
                node.status === "active" ? "is-active" : node.status === "paused" ? "is-warn" : "";
              return (
                <div
                  class={`colony-card ${statusClass()}`}
                  role="button"
                  tabindex="0"
                  onClick={() => props.onSelect?.(node.id)}
                >
                  <div style={{ width: `${size.width / 2}px`, height: "26px", position: "relative", "flex-shrink": "0" }}>
                    <div class={`pixel-sprite ${spriteClass}`} style={{ transform: "scale(2)" }} />
                  </div>
                  <div class="colony-text-3xs font-bold text-text-primary truncate w-full">
                    {node.label.replace("connector-", "")}
                  </div>
                  <div class={`colony-text-5xs uppercase ${
                    node.status === "active" ? "text-status-ok" : node.status === "paused" ? "text-status-warn" : "text-text-dim"
                  }`}>
                    {node.status}
                  </div>
                </div>
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
              const nodeType = ["conifer", "deciduous", "shrub"][seeded(connector.name + "type", 0, 3)] ?? "deciduous";
              const spriteClass = NODE_SPRITES[nodeType as keyof typeof NODE_SPRITES] ?? "sprite-tree-deciduous";
              const size = NODE_SIZES[nodeType] ?? { width: 36, height: 44 };
              return (
                <div
                  class="colony-card is-available"
                  role="button"
                  tabindex="0"
                  onClick={() => props.onSetup?.(connector.name)}
                >
                  <div style={{ width: `${size.width / 2}px`, height: "26px", position: "relative", "flex-shrink": "0" }}>
                    <div class={`pixel-sprite ${spriteClass}`} style={{ transform: "scale(2)", opacity: 0.5 }} />
                  </div>
                  <div class="colony-text-3xs text-text-secondary truncate w-full">
                    {connector.name.replace("connector-", "")}
                  </div>
                  <div class="colony-text-5xs text-status-ok">
                    {connector.requires_config ? "Configure" : "Enable"}
                  </div>
                </div>
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
