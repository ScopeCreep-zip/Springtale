import type { Component } from "solid-js";
import { For } from "solid-js";
import type { EventItem } from "../dashboard/model";
import type { ColonyAgent, ColonyFormation, ColonyNode, ColonySelection } from "./types";

export interface TopBarProps {
  agents: ColonyAgent[];
  nodes: ColonyNode[];
  formations: ColonyFormation[];
  events: EventItem[];
  selection: ColonySelection;
  onSelectAgent: (id: string) => void;
  onSelectFormation: (id: string) => void;
}

/**
 * Top Bar — cadence waveform + agent roster slots + colony summary.
 *
 * Sources: RimWorld colonist bar, ONI colony summary, pixel waveform.
 */
/** Cadence histogram width — one bucket per second. */
const CADENCE_BUCKETS = 20;
/** Tallest bar, in px; the bar strip is 16px high. */
const CADENCE_MAX_PX = 14;

/**
 * Plan 3.3 — the roster glyph reports what the agent is doing:
 * `-` idle, `!` firing right now, `*` otherwise. It is deliberately NOT a
 * fuel readout: `fuel < 20` is true of every disabled agent, so the old
 * rule painted the whole roster with alarms.
 */
const rosterGlyph = (agent: ColonyAgent) =>
  agent.status === "idle" ? "-" : agent.activity === "firing" ? "!" : "*";

export const TopBar: Component<TopBarProps> = (props) => {
  /**
   * Cadence waveform — a real events-per-second histogram over the last
   * `CADENCE_BUCKETS` seconds of the event stream, newest bucket on the
   * right. Buckets are keyed off the newest event's own timestamp (not the
   * wall clock) so the shape matches the data the panel is showing.
   */
  const cadence = () => {
    const buckets: number[] = new Array(CADENCE_BUCKETS).fill(0);
    const stamps = props.events.map((e) => Date.parse(e.timestamp)).filter(Number.isFinite);
    if (stamps.length === 0) return buckets;
    const newest = Math.max(...stamps);
    for (const t of stamps) {
      const age = Math.floor((newest - t) / 1000);
      if (age >= 0 && age < CADENCE_BUCKETS) {
        const i = CADENCE_BUCKETS - 1 - age;
        buckets[i] = (buckets[i] ?? 0) + 1;
      }
    }
    return buckets;
  };
  /** Scale to the busiest bucket so the shape stays readable at any volume. */
  const barHeight = (count: number) => {
    const peak = Math.max(1, ...cadence());
    return 1 + Math.round((count / peak) * (CADENCE_MAX_PX - 1));
  };

  const liveCount = () => props.agents.filter((a) => a.status !== "idle").length;
  const nodeCount = () => props.nodes.length;
  const guardStatus = () => {
    const hasActive = props.agents.some((a) => a.status === "ok");
    return hasActive ? "OK" : "--";
  };

  return (
    <>
      {/* Cadence waveform */}
      <div class="mr-1.5 flex h-4 shrink-0 items-end gap-px">
        <For each={cadence()}>
          {(count) => (
            <div class="colony-cadence-bar" style={{ "--colony-h": `${barHeight(count)}px` }} />
          )}
        </For>
      </div>

      {/* Agent roster */}
      <div class="flex flex-1 items-center gap-0.5 overflow-x-auto py-0.5 scrollbar-none">
        <For each={props.formations}>
          {(formation) => (
            <button
              type="button"
              class="mx-0.5 flex items-center gap-0 border border-bark-light px-0.5 py-px"
              onClick={() => props.onSelectFormation(formation.id)}
            >
              <span
                class="colony-vertical-label colony-tinted mr-0.5"
                style={{ "--colony-color": formation.color }}
              >
                {formation.name.split(" ")[0]}
              </span>
              <For
                each={props.agents.filter(
                  (a) => a.connectorId && formation.members.includes(a.connectorId),
                )}
              >
                {(agent) => (
                  <button
                    type="button"
                    class={`colony-roster-slot ${props.selection.id === agent.id ? "is-selected" : ""}`}
                    data-status={
                      agent.status === "ok" ? "ok" : agent.status === "warn" ? "warn" : "idle"
                    }
                    title={`${agent.name}: ${agent.task}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      props.onSelectAgent(agent.id);
                    }}
                  >
                    <span>{rosterGlyph(agent)}</span>
                  </button>
                )}
              </For>
            </button>
          )}
        </For>

        <For
          each={props.agents.filter(
            (a) =>
              !props.formations.some((f) => a.connectorId && f.members.includes(a.connectorId)),
          )}
        >
          {(agent) => (
            <button
              type="button"
              class={`colony-roster-slot ${props.selection.id === agent.id ? "is-selected" : ""}`}
              data-status={agent.status === "ok" ? "ok" : agent.status === "warn" ? "warn" : "idle"}
              title={`${agent.name}: ${agent.task}`}
              onClick={() => props.onSelectAgent(agent.id)}
            >
              <span>{rosterGlyph(agent)}</span>
            </button>
          )}
        </For>
      </div>

      {/* Colony summary */}
      <div class="colony-text-sm ml-auto flex shrink-0 gap-2.5">
        <div class="flex items-center gap-0.5">
          <span class="text-status-ok">*</span>
          <span class="colony-value">{liveCount()}</span>
          <span class="text-text-dim">LIVE</span>
        </div>
        <div class="flex items-center gap-0.5">
          <span class="text-canopy">^</span>
          <span class="colony-value">{nodeCount()}</span>
          <span class="text-text-dim">NODES</span>
        </div>
        <div class="flex items-center gap-0.5">
          <span class="text-status-ok">+</span>
          <span
            class={`colony-value ${guardStatus() === "OK" ? "text-status-ok" : "text-text-dim"}`}
          >
            {guardStatus()}
          </span>
          <span class="text-text-dim">GUARD</span>
        </div>
      </div>
    </>
  );
};
