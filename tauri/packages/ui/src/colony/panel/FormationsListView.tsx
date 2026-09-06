import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import type { ColonyAgent, ColonyFormation } from "../types";

export const FormationsListView: Component<{
  formations: ColonyFormation[];
  agents: ColonyAgent[];
  addAgentId?: string;
  onAddToFormation?: (formationId: string, connectorName: string) => Promise<void>;
}> = (props) => (
  <div>
    <div class="colony-label mb-1">
      {props.addAgentId ? "SELECT FORMATION TO JOIN" : `FORMATIONS (${props.formations.length})`}
    </div>
    <Show
      when={props.formations.length > 0}
      fallback={
        <p class="colony-text-xs py-2 text-text-dim">No formations. Use NEW RULE to create one.</p>
      }
    >
      <div class="space-y-1">
        <For each={props.formations}>
          {(formation) => {
            const memberCount = formation.members.length;
            return (
              <button
                type="button"
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
                <span
                  class="colony-tinted colony-text-xs font-bold"
                  style={{ "--colony-color": formation.color }}
                >
                  {formation.name}
                </span>
                <span class="colony-text-3xs uppercase text-text-dim">{formation.intent}</span>
                <span class="colony-text-3xs ml-auto text-text-dim">{memberCount} members</span>
                <span
                  class="colony-tinted colony-text-3xs font-bold"
                  style={{ "--colony-color": formation.color }}
                >
                  {formation.momentumLabel}
                </span>
              </button>
            );
          }}
        </For>
      </div>
    </Show>
  </div>
);
