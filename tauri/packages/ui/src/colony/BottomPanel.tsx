import { For, Show, Switch, Match } from "solid-js";
import type { Component, JSX } from "solid-js";
import type {
  ColonyTree, ColonyAgent, ColonyConnection, ColonyFormation, ColonySelection, ColonyCommand,
} from "./types";
import {
  COMMANDS, ROLE_SPRITES, ROLE_COLORS, AUTONOMY_LABELS,
  MOMENTUM_NAMES, MOMENTUM_COLORS, MOMENTUM_UNLOCKS, seeded,
} from "./types";

export interface BottomPanelProps {
  trees: ColonyTree[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  selection: ColonySelection;
  onCommand: (label: string) => void;
  /** Optional custom detail content (for hatch wizard, settings, etc.) */
  detailOverride?: JSX.Element;
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
        <div class="text-text-dim" style={{ "font-size": "5px", "letter-spacing": "2px", "margin-bottom": "3px" }}>
          COLONY MAP
        </div>
        <Minimap
          trees={props.trees}
          agents={props.agents}
          connections={props.connections}
          formations={props.formations}
          selection={props.selection}
        />
      </div>

      {/* Zone 2: Detail Panel */}
      <div class="overflow-y-auto p-1.5 px-2.5" style={{ "scrollbar-width": "thin", "scrollbar-color": "var(--color-bark) transparent" }}>
        <Show when={props.detailOverride} fallback={
          <DetailPanel
            trees={props.trees}
            agents={props.agents}
            connections={props.connections}
            formations={props.formations}
            selection={props.selection}
          />
        }>
          {props.detailOverride}
        </Show>
      </div>

      {/* Zone 3: Command Grid */}
      <div class="border-l-2 border-bark p-1.5">
        <div class="text-text-dim" style={{ "font-size": "5px", "letter-spacing": "2px", "margin-bottom": "3px" }}>
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
  trees: ColonyTree[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  selection: ColonySelection;
}> = (props) => {
  const getAgentPos = (agent: ColonyAgent) => {
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

  return (
    <div class="colony-minimap">
      {/* Formation zones */}
      <For each={props.formations}>
        {(f) => (
          <div
            class="absolute rounded-[40%] opacity-30"
            style={{
              left: `${f.zone.x - 6}%`, top: `${f.zone.y - 4}%`,
              width: "12%", height: "8%",
              border: `1px solid ${f.color}`,
            }}
          />
        )}
      </For>

      {/* Trees */}
      <For each={props.trees}>
        {(tree) => (
          <div
            class="colony-minimap-tree"
            style={{
              left: `${tree.x - 1}%`, top: `${tree.y - 1}%`,
              width: "4px", height: "4px",
              background: tree.status === "active" ? "var(--color-canopy)"
                : tree.status === "paused" ? "var(--color-canopy-degraded)" : "#333",
            }}
          />
        )}
      </For>

      {/* Connections */}
      <For each={props.connections}>
        {(conn) => {
          const treeA = () => props.trees.find((t) => t.id === conn.a);
          const treeB = () => props.trees.find((t) => t.id === conn.b);
          const a = treeA();
          const b = treeB();
          if (!a || !b) return null;
          const dx = b.x - a.x;
          const dy = b.y - a.y;
          const length = Math.sqrt(dx * dx + dy * dy);
          const angle = (Math.atan2(dy, dx) * 180) / Math.PI;
          const hasActive = conn.pipes.some((p) => p.status === "active");
          return (
            <div
              class="colony-minimap-line"
              style={{
                left: `${a.x}%`, top: `${a.y}%`,
                width: `${length}%`,
                transform: `rotate(${angle}deg)`, "transform-origin": "0 0",
                background: hasActive ? "var(--color-mycelium-active)" : "var(--color-mycelium)",
              }}
            />
          );
        }}
      </For>

      {/* Agents */}
      <For each={props.agents}>
        {(agent) => {
          const pos = getAgentPos(agent);
          const isSelected = props.selection.id === agent.id;
          return (
            <div
              class="colony-minimap-dot"
              style={{
                left: `${pos.x}%`, top: `${pos.y}%`,
                background: ROLE_COLORS[agent.role] ?? "var(--color-text-secondary)",
                ...(isSelected ? { width: "5px", height: "5px" } : {}),
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
  trees: ColonyTree[];
  agents: ColonyAgent[];
  connections: ColonyConnection[];
  formations: ColonyFormation[];
  selection: ColonySelection;
}> = (props) => {
  return (
    <Switch fallback={
      <div>
        <div class="text-text-dim" style={{ "font-size": "5px", "letter-spacing": "2px", "margin-bottom": "4px" }}>COLONY</div>
        <div class="py-3 text-center text-text-dim" style={{ "font-size": "7px" }}>
          Click a tree, springtail, or formation
        </div>
      </div>
    }>
      {/* Agent detail */}
      <Match when={props.selection.type === "agent"}>
        {(() => {
          const agent = () => props.agents.find((a) => a.id === props.selection.id);
          const formation = () => props.formations.find((f) => f.members.includes(props.selection.id ?? ""));
          if (!agent()) return null;
          const a = agent()!;
          const fuelColor = a.fuel > 50 ? "var(--color-status-ok)" : "var(--color-status-warn)";

          return (
            <div>
              <div class="text-text-dim" style={{ "font-size": "5px", "letter-spacing": "2px", "margin-bottom": "4px" }}>SPRINGTAIL</div>
              <div class="flex items-start gap-2">
                <div class="colony-portrait-frame">
                  <div class={`pixel-sprite ${ROLE_SPRITES[a.role]}`} style={{ transform: "scale(3)" }} />
                </div>
                <div class="min-w-0 flex-1">
                  <div class="font-bold" style={{ "font-size": "9px" }}>{a.name}</div>
                  <div class="uppercase text-text-dim" style={{ "font-size": "6px", "letter-spacing": "1px" }}>
                    {a.role}{a.treeId ? ` near ${a.treeId}` : " roaming"}
                    {formation() ? ` / ${formation()!.name}` : ""}
                  </div>
                  <div class="mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-status-warn" style={{ "font-size": "7px" }}>
                    {a.task}
                  </div>
                  {/* Stat bars */}
                  <div class="mt-1 grid items-center gap-x-1 gap-y-px" style={{ "grid-template-columns": "24px 1fr 20px" }}>
                    <span class="text-text-dim" style={{ "font-size": "5px" }}>FUEL</span>
                    <div class="colony-stat-bar">
                      <div class="colony-stat-fill" style={{ width: `${a.fuel}%`, background: fuelColor }} />
                    </div>
                    <span class="text-right" style={{ "font-size": "6px", color: fuelColor }}>{a.fuel}</span>
                    <span class="text-text-dim" style={{ "font-size": "5px" }}>HP</span>
                    <div class="colony-stat-bar">
                      <div class="colony-stat-fill" style={{ width: `${a.hp}%`, background: "var(--color-role-scout)" }} />
                    </div>
                    <span class="text-right text-role-scout" style={{ "font-size": "6px" }}>{a.hp}</span>
                  </div>
                  <Show when={a.pipeline}>
                    <div class="mt-0.5 text-mycelium-active" style={{ "font-size": "6px" }}>
                      PIPELINE: {a.pipeline}
                    </div>
                  </Show>
                  {/* Autonomy pips */}
                  <div class="mt-1 flex gap-0.5">
                    <For each={[0, 1, 2, 3, 4]}>
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
                            color: a.autonomy === level ? "#000" : undefined,
                          }}
                        >
                          {level}
                        </div>
                      )}
                    </For>
                  </div>
                  <div class="mt-0.5 text-text-dim" style={{ "font-size": "5px" }}>
                    {AUTONOMY_LABELS[a.autonomy]}
                  </div>
                </div>
              </div>
            </div>
          );
        })()}
      </Match>

      {/* Tree detail */}
      <Match when={props.selection.type === "tree"}>
        {(() => {
          const tree = () => props.trees.find((t) => t.id === props.selection.id);
          if (!tree()) return null;
          const t = tree()!;
          const relatedConns = () => props.connections.filter((c) => c.a === t.id || c.b === t.id);
          const nearbyAgents = () => props.agents.filter((a) => a.treeId === t.id);
          const statusColor = t.status === "active" ? "var(--color-status-ok)" : t.status === "paused" ? "var(--color-status-warn)" : "var(--color-text-dim)";

          return (
            <div>
              <div class="text-text-dim" style={{ "font-size": "5px", "letter-spacing": "2px", "margin-bottom": "4px" }}>CONNECTOR</div>
              <div class="font-bold" style={{ "font-size": "9px" }}>{t.label}</div>
              <div class="uppercase" style={{ "font-size": "6px", "letter-spacing": "1px", color: statusColor }}>
                {t.status.toUpperCase()} {t.type.toUpperCase()}
              </div>
              <div class="mt-1.5 text-text-dim" style={{ "font-size": "6px" }}>MYCELIUM</div>
              <For each={relatedConns()}>
                {(conn) => (
                  <For each={conn.pipes}>
                    {(pipe) => {
                      const pipeColor = pipe.status === "active" ? "var(--color-mycelium-active)" : "var(--color-text-dim)";
                      const direction = pipe.dir === 1 ? `${conn.a} > ${conn.b}` : `${conn.b} > ${conn.a}`;
                      return (
                        <div class="flex justify-between border-b border-bark py-0.5" style={{ "font-size": "6px" }}>
                          <span style={{ color: pipeColor }}>{pipe.id}</span>
                          <span>{direction}</span>
                        </div>
                      );
                    }}
                  </For>
                )}
              </For>
              <div class="mt-1.5 text-text-dim" style={{ "font-size": "6px" }}>NEARBY SPRINGTAILS</div>
              <For each={nearbyAgents()}>
                {(a) => (
                  <div class="flex justify-between border-b border-bark py-0.5" style={{ "font-size": "6px" }}>
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
          const members = () => f.members.map((id) => props.agents.find((a) => a.id === id)).filter(Boolean) as ColonyAgent[];

          return (
            <div>
              <div class="text-text-dim" style={{ "font-size": "5px", "letter-spacing": "2px", "margin-bottom": "4px" }}>FORMATION</div>
              <div class="font-bold" style={{ "font-size": "9px", color: f.color }}>{f.name}</div>
              <div class="text-text-dim" style={{ "font-size": "6px" }}>{f.intent}: {f.description}</div>
              {/* Momentum bar */}
              <div class="my-1.5 flex gap-0.5">
                <For each={MOMENTUM_NAMES}>
                  {(_, i) => (
                    <div
                      class="h-[5px] flex-1"
                      style={{
                        background: i() <= f.momentum ? MOMENTUM_COLORS[i()] : "#120f08",
                      }}
                    />
                  )}
                </For>
              </div>
              <div class="mb-1" style={{ "font-size": "6px", color: f.color }}>
                {f.momentumLabel} — {MOMENTUM_UNLOCKS[f.momentum]}
              </div>
              <For each={members()}>
                {(a) => {
                  const fuelColor = a.fuel > 50 ? "var(--color-status-ok)" : "var(--color-status-warn)";
                  return (
                    <div class="flex justify-between border-b border-bark py-0.5" style={{ "font-size": "6px" }}>
                      <span>* {a.name} <span class="text-text-dim">{a.role}</span></span>
                      <span style={{ color: fuelColor }}>{a.fuel} fuel</span>
                    </div>
                  );
                }}
              </For>
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
              onClick={() => props.onCommand(cmd.label)}
            >
              <span style={{ "font-size": "10px" }}>{cmd.icon}</span>
              {cmd.label}
              <span class="bg-soil-deep px-0.5 text-text-dim" style={{ "font-size": "5px" }}>{cmd.key}</span>
            </button>
          );
        }}
      </For>
    </div>
  );
};
