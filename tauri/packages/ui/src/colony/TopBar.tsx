import { For, createSignal, createEffect } from "solid-js";
import type { Component } from "solid-js";
import type { ColonyAgent, ColonyNode, ColonyFormation, ColonySelection } from "./types";
import type { EventItem } from "../dashboard/model";

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
export const TopBar: Component<TopBarProps> = (props) => {
  // Cadence waveform — derives from real event activity.
  // Each bar = number of events in a recent time slice.
  // More events = taller bars. Idle colony = flat bars.
  const [bars, setBars] = createSignal<number[]>(Array(20).fill(2));

  createEffect(() => {
    const eventCount = props.events.length;
    const activeAgents = props.agents.filter((a) => a.status === "ok").length;
    // Push a new bar based on real activity level
    const activity = Math.min(14, 2 + eventCount + activeAgents * 2);
    setBars((prev) => {
      const next = [...prev];
      next.shift();
      next.push(activity);
      return next;
    });
  });

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
        <For each={bars()}>
          {(h) => <div class="colony-cadence-bar" style={{ height: `${h}px` }} />}
        </For>
      </div>

      {/* Agent roster */}
      <div class="flex flex-1 items-center gap-0.5 overflow-x-auto py-0.5 scrollbar-none">
        <For each={props.formations}>
          {(formation) => (
            <div
              class="mx-0.5 flex items-center gap-0 border border-bark-light px-0.5 py-px"
              onClick={() => props.onSelectFormation(formation.id)}
            >
              <span class="colony-vertical-label mr-0.5" style={{ color: formation.color }}>
                {formation.name.split(" ")[0]}
              </span>
              <For each={props.agents.filter((a) => a.connectorId && formation.members.includes(a.connectorId))}>
                {(agent) => (
                  <div
                    class={`colony-roster-slot ${props.selection.id === agent.id ? "is-selected" : ""}`}
                    data-status={agent.status === "ok" ? "ok" : agent.status === "warn" ? "warn" : "idle"}
                    title={`${agent.name}: ${agent.task}`}
                    onClick={(e) => { e.stopPropagation(); props.onSelectAgent(agent.id); }}
                  >
                    <span>{agent.fuel < 20 ? "!" : agent.status === "idle" ? "-" : "*"}</span>
                  </div>
                )}
              </For>
            </div>
          )}
        </For>

        <For each={props.agents.filter((a) => !props.formations.some((f) => a.connectorId && f.members.includes(a.connectorId)))}>
          {(agent) => (
            <div
              class={`colony-roster-slot ${props.selection.id === agent.id ? "is-selected" : ""}`}
              data-status={agent.status === "ok" ? "ok" : agent.status === "warn" ? "warn" : "idle"}
              title={`${agent.name}: ${agent.task}`}
              onClick={() => props.onSelectAgent(agent.id)}
            >
              <span>{agent.fuel < 20 ? "!" : agent.status === "idle" ? "-" : "*"}</span>
            </div>
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
          <span class={`colony-value ${guardStatus() === "OK" ? "text-status-ok" : "text-text-dim"}`}>{guardStatus()}</span>
          <span class="text-text-dim">GUARD</span>
        </div>
      </div>
    </>
  );
};
