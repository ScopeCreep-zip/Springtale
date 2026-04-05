import { For, createSignal, onMount, onCleanup } from "solid-js";
import type { Component } from "solid-js";
import type { ColonyAgent, ColonyTree, ColonyFormation, ColonySelection } from "./types";

export interface TopBarProps {
  agents: ColonyAgent[];
  trees: ColonyTree[];
  formations: ColonyFormation[];
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
  // Cadence waveform — 20 bars animated
  const [bars, setBars] = createSignal<number[]>(Array(20).fill(3));

  let cadenceTimer: ReturnType<typeof setInterval>;
  onMount(() => {
    cadenceTimer = setInterval(() => {
      setBars((prev) => {
        const next = [...prev];
        next.shift();
        next.push(2 + Math.floor(Math.random() * 12));
        return next;
      });
    }, 600);
  });
  onCleanup(() => clearInterval(cadenceTimer));

  const liveCount = () => props.agents.filter((a) => a.status !== "idle").length;
  const treeCount = () => props.trees.length;
  const guardStatus = () => {
    const sentinel = props.agents.find((a) => a.role === "sentinel");
    return sentinel ? (sentinel.status === "ok" ? "OK" : "WARN") : "--";
  };

  return (
    <>
      {/* Cadence waveform */}
      <div class="mr-1.5 flex shrink-0 items-end gap-px" style={{ height: "16px" }}>
        <For each={bars()}>
          {(h) => <div class="colony-cadence-bar" style={{ height: `${h}px` }} />}
        </For>
      </div>

      {/* Agent roster — scrollable row of slots */}
      <div class="flex flex-1 items-center gap-0.5 overflow-x-auto py-0.5 scrollbar-none">
        {/* Formation groups */}
        <For each={props.formations}>
          {(formation) => (
            <div
              class="mx-0.5 flex items-center gap-0 border border-bark-light px-0.5 py-px"
              onClick={() => props.onSelectFormation(formation.id)}
            >
              <span
                class="mr-0.5 text-text-dim"
                style={{ "font-size": "5px", "writing-mode": "vertical-rl", "letter-spacing": "0.5px", color: formation.color }}
              >
                {formation.name.split(" ")[0]}
              </span>
              <For each={formation.members}>
                {(memberId) => {
                  const agent = () => props.agents.find((a) => a.id === memberId);
                  return (
                    <div
                      class={`colony-roster-slot ${props.selection.id === memberId ? "is-selected" : ""}`}
                      data-status={agent()?.status === "ok" ? "ok" : agent()?.status === "warn" ? "warn" : "idle"}
                      title={agent() ? `${agent()!.name}: ${agent()!.task}` : ""}
                      onClick={(e) => { e.stopPropagation(); if (agent()) props.onSelectAgent(agent()!.id); }}
                    >
                      <span>{agent()?.fuel !== undefined && agent()!.fuel < 20 ? "!" : agent()?.status === "idle" ? "-" : "*"}</span>
                    </div>
                  );
                }}
              </For>
            </div>
          )}
        </For>

        {/* Unformed agents */}
        <For each={props.agents.filter((a) => !props.formations.some((f) => f.members.includes(a.id)))}>
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

      {/* Colony summary — ONI 4-icon pattern */}
      <div class="ml-auto flex shrink-0 gap-2.5" style={{ "font-size": "8px" }}>
        <div class="flex items-center gap-0.5">
          <span class="text-status-ok">*</span>
          <span class="font-bold" style={{ "font-size": "9px" }}>{liveCount()}</span>
          <span class="text-text-dim">LIVE</span>
        </div>
        <div class="flex items-center gap-0.5">
          <span class="text-canopy">^</span>
          <span class="font-bold" style={{ "font-size": "9px" }}>{treeCount()}</span>
          <span class="text-text-dim">TREES</span>
        </div>
        <div class="flex items-center gap-0.5">
          <span class="text-status-ok">+</span>
          <span class="font-bold" style={{ "font-size": "9px" }}>{guardStatus()}</span>
          <span class="text-text-dim">GUARD</span>
        </div>
      </div>
    </>
  );
};
