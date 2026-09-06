import type { Component } from "solid-js";
import { For, Match, Show, Switch } from "solid-js";
import type {
  ColonyAgent,
  ColonyConnection,
  ColonyFormation,
  ColonyNode,
  ColonySelection,
} from "../types";
import {
  AUTONOMY_COLORS,
  MOMENTUM_COLORS,
  MOMENTUM_NAMES,
  MOMENTUM_UNLOCKS,
  ROLE_COLORS,
  ROLE_SPRITES,
  TIER_CAPABILITIES,
} from "../types";
import { FormationAiAdapterRow } from "./FormationAiAdapterRow";
import { FormationEventLog } from "./FormationEventLog";
import { healthColor } from "./health";

export const DetailPanel: Component<{
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
    <Switch
      fallback={
        <div>
          <div class="colony-label mb-1 text-text-dim">COLONY</div>
          <div class="colony-text-xs py-3 text-center text-text-dim">
            Click a node, agent, or formation
          </div>
        </div>
      }
    >
      {/* Agent detail */}
      <Match when={props.selection.type === "agent"}>
        {(() => {
          const agent = () => props.agents.find((a) => a.id === props.selection.id);
          const formation = () => {
            const a = agent();
            if (!a?.connectorId) return undefined;
            const connectorId = a.connectorId;
            return props.formations.find((f) => f.members.includes(connectorId));
          };
          const a = agent();
          if (!a) return null;
          const fuelColor =
            a.fuelStatus === "ok"
              ? "var(--color-status-ok)"
              : a.fuelStatus === "warn"
                ? "var(--color-status-warn)"
                : "var(--color-status-error)";

          return (
            <div>
              <div class="colony-label mb-1 text-text-dim">AGENT</div>
              <div class="flex items-start gap-2">
                <div class="colony-portrait-frame">
                  <div
                    class={`pixel-sprite ${ROLE_SPRITES[a.role]}`}
                    style={{ transform: "scale(3)" }}
                  />
                </div>
                <div class="min-w-0 flex-1">
                  <div class="colony-text-md font-bold">{a.name}</div>
                  <div class="colony-text-2xs uppercase tracking-[1px] text-text-dim">
                    {a.role}
                    {a.connectorId ? ` near ${a.connectorId}` : " roaming"}
                    {formation() ? ` / ${formation()?.name}` : ""}
                  </div>
                  <div class="colony-text-xs mt-0.5 overflow-hidden text-ellipsis whitespace-nowrap text-status-warn">
                    {a.task}
                  </div>
                  {/* Health badge */}
                  <div
                    class="colony-tinted colony-text-xs mt-0.5 uppercase tracking-wider"
                    style={{ "--colony-color": healthColor(a.healthState) }}
                  >
                    {a.healthState ?? "healthy"} {(a.liveness ?? 1) < 0.5 ? `| SUSPECT` : ""}
                  </div>
                  {/* Stat bars */}
                  <div class="colony-stat-grid mt-1">
                    <span class="colony-text-3xs text-text-dim">FUEL</span>
                    <div class="colony-stat-bar">
                      <div
                        class="colony-stat-fill"
                        style={{ width: `${a.fuel}%`, "--colony-bg": fuelColor }}
                      />
                    </div>
                    <span
                      class="colony-tinted colony-text-xs text-right"
                      style={{ "--colony-color": fuelColor }}
                    >
                      {a.fuel}
                    </span>
                    <span class="colony-text-3xs text-text-dim">HP</span>
                    <div class="colony-stat-bar">
                      <div
                        class="colony-stat-fill"
                        style={{
                          width: `${a.hp}%`,
                          "--colony-bg": healthColor(a.healthState, "var(--color-role-scout)"),
                        }}
                      />
                    </div>
                    <span
                      class="colony-tinted colony-text-2xs text-right"
                      style={{
                        "--colony-color": healthColor(a.healthState, "var(--color-role-scout)"),
                      }}
                    >
                      {a.hp}
                    </span>
                    <span class="colony-text-3xs text-text-dim">LOAD</span>
                    <div class="colony-stat-bar">
                      <div
                        class="colony-stat-fill"
                        style={{
                          width: `${Math.round((a.attentionLoad ?? 0) * 100)}%`,
                          "--colony-bg":
                            (a.attentionLoad ?? 0) > 0.8
                              ? "var(--color-status-error)"
                              : (a.attentionLoad ?? 0) > 0.5
                                ? "var(--color-status-warn)"
                                : "var(--color-status-ok)",
                        }}
                      />
                    </div>
                    <span class="colony-text-xs text-right text-text-dim">
                      {Math.round((a.attentionLoad ?? 0) * 100)}%
                    </span>
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
                          classList={{ "is-active": a.autonomy === level }}
                          style={{
                            // One colour per `AUTONOMY_LABELS` entry:
                            // observe → suggest → approve → autonomous.
                            // A fifth colour outlived the SELF-DIRECT level
                            // that was cut from the label list.
                            "--colony-color": AUTONOMY_COLORS[level],
                          }}
                        >
                          {level}
                        </div>
                      )}
                    </For>
                  </div>
                  <div class="colony-text-3xs mt-0.5 text-text-dim">{a.autonomyLabel}</div>
                </div>
              </div>
            </div>
          );
        })()}
      </Match>

      {/* Tree detail */}
      <Match when={props.selection.type === "connector"}>
        {(() => {
          const t = props.nodes.find((node) => node.id === props.selection.id);
          if (!t) return null;
          const relatedConns = () => props.connections.filter((c) => c.a === t.id || c.b === t.id);
          const nearbyAgents = () => props.agents.filter((a) => a.connectorId === t.id);
          const statusColor =
            t.status === "active"
              ? "var(--color-status-ok)"
              : t.status === "paused"
                ? "var(--color-status-warn)"
                : "var(--color-text-dim)";

          return (
            <div>
              <div class="colony-label mb-1 text-text-dim">CONNECTOR</div>
              <div class="colony-text-md font-bold">{t.label}</div>
              <div
                class="colony-tinted colony-text-xs uppercase tracking-wider"
                style={{ "--colony-color": statusColor }}
              >
                {t.status.toUpperCase()} {t.type.toUpperCase()}
              </div>
              <div class="colony-text-2xs mt-1.5 text-text-dim">CONNECTIONS</div>
              <For each={relatedConns()}>
                {(conn) => (
                  <For each={conn.pipes}>
                    {(pipe) => {
                      const pipeColor =
                        pipe.status === "active"
                          ? "var(--color-mycelium-active)"
                          : "var(--color-text-dim)";
                      const direction =
                        pipe.dir === 1 ? `${conn.a} > ${conn.b}` : `${conn.b} > ${conn.a}`;
                      return (
                        <div class="colony-text-2xs flex justify-between border-b border-bark py-0.5">
                          <span class="colony-tinted" style={{ "--colony-color": pipeColor }}>
                            {pipe.id}
                          </span>
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
          const f = props.formations.find((formation) => formation.id === props.selection.id);
          if (!f) return null;
          const members = () =>
            props.agents.filter((a) => a.connectorId && f.members.includes(a.connectorId));

          return (
            <div>
              <div class="colony-label mb-1 text-text-dim">FORMATION</div>
              <div class="flex items-center gap-2">
                <div
                  class="colony-tinted colony-text-2xs font-bold"
                  style={{ "--colony-color": f.color }}
                >
                  {f.name}
                </div>
                <Show when={f.status === "paused"}>
                  <span class="colony-text-xs rounded bg-status-warn px-1 text-soil-deep">
                    PAUSED
                  </span>
                </Show>
                <Show when={f.status === "draft"}>
                  <span class="colony-text-xs rounded border border-bark px-1 text-text-dim">
                    DRAFT
                  </span>
                </Show>
                <Show when={f.guardStatus === "GUARD"}>
                  <span class="colony-text-xs rounded bg-status-ok px-1 text-soil-deep">GUARD</span>
                </Show>
              </div>
              <div class="colony-text-2xs text-text-dim">
                {f.intent}: {f.description}
              </div>

              {/* Momentum bar */}
              <div class="my-1.5 flex gap-0.5">
                <For each={MOMENTUM_NAMES}>
                  {(_, i) => (
                    <div
                      class="colony-momentum-seg h-[5px] flex-1"
                      style={{
                        "--colony-bg":
                          i() <= f.momentum ? MOMENTUM_COLORS[i()] : "var(--color-soil-darker)",
                      }}
                    />
                  )}
                </For>
              </div>
              <div class="colony-tinted colony-text-xs mb-1" style={{ "--colony-color": f.color }}>
                {f.momentumLabel} — {MOMENTUM_UNLOCKS[f.momentum]}
              </div>

              {/* Rally pips (Monster Hunter carts) — only once a rally budget exists */}
              <Show when={f.rallyMax > 0}>
                <div class="mb-1 flex items-center gap-1">
                  <span class="colony-text-3xs text-text-dim">RALLY</span>
                  <div class="flex gap-0.5">
                    <For each={Array.from({ length: f.rallyMax })}>
                      {(_, i) => (
                        <div
                          class="colony-panel-pip h-[6px] w-[6px] rounded-sm"
                          classList={{ "is-filled": i() < f.rallyTokens }}
                        />
                      )}
                    </For>
                  </div>
                  <span class="colony-text-xs text-text-dim">
                    {f.rallyTokens}/{f.rallyMax}
                  </span>
                </div>
              </Show>

              {/* Aggregate stats row */}
              {(() => {
                const operational = members().filter(
                  (a) => a.healthState === "healthy" || a.healthState === "degraded",
                ).length;
                const avgLoad =
                  members().length > 0
                    ? Math.round(
                        (members().reduce((sum, a) => sum + (a.attentionLoad ?? 0), 0) /
                          members().length) *
                          100,
                      )
                    : 0;
                const totalFuel = members().reduce((sum, a) => sum + a.fuel, 0);
                return (
                  <div class="colony-text-xs mb-1 flex gap-2 text-text-dim">
                    <span>
                      {operational}/{members().length} OPS
                    </span>
                    <span>LOAD {avgLoad}%</span>
                    <span>FUEL {totalFuel}</span>
                  </div>
                );
              })()}

              {/* Member traffic-light grid (SC2 wireframe pattern) */}
              <div class="colony-label mb-0.5">MEMBERS</div>
              <For each={members()}>
                {(a) => {
                  const memberColor = healthColor(a.healthState);
                  const livenessIcon =
                    (a.liveness ?? 1) > 0.8 ? "●" : (a.liveness ?? 1) > 0.3 ? "◐" : "○";
                  const fuelColor =
                    a.fuelStatus === "ok"
                      ? "var(--color-status-ok)"
                      : a.fuelStatus === "warn"
                        ? "var(--color-status-warn)"
                        : "var(--color-status-error)";
                  return (
                    <div class="colony-text-2xs flex justify-between border-b border-bark py-0.5">
                      <span>
                        <span class="colony-tinted" style={{ "--colony-color": memberColor }}>
                          {livenessIcon}
                        </span>{" "}
                        {a.name} <span class="text-text-dim">{a.role}</span>
                      </span>
                      <span class="flex gap-1.5">
                        <Show when={(a.attentionLoad ?? 0) > 0.5}>
                          <span class="text-status-warn">
                            {Math.round((a.attentionLoad ?? 0) * 100)}%
                          </span>
                        </Show>
                        <span class="colony-tinted" style={{ "--colony-color": fuelColor }}>
                          {a.fuel}
                        </span>
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
                          class="colony-attn-seg"
                          style={{
                            width: `${Math.max(share, 5)}%`,
                            "--colony-bg": ROLE_COLORS[a.role] ?? "var(--color-text-dim)",
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
                    <span class="colony-text-xs rounded border border-bark bg-soil-deep px-1.5 py-0.5 text-text-secondary">
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
