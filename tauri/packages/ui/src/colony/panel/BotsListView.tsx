import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import type { ColonyAgent } from "../types";
import { ROLE_COLORS, ROLE_SPRITES } from "../types";

export const BotsListView: Component<{
  agents: ColonyAgent[];
  onSelect?: (id: string) => void;
  onCreateNew?: () => void;
}> = (props) => (
  <div>
    <div class="colony-label mb-1">BOTS ({props.agents.length})</div>
    <Show
      when={props.agents.length > 0 || props.onCreateNew}
      fallback={<p class="colony-text-xs py-2 text-text-dim">No bots yet.</p>}
    >
      <div class="colony-card-strip">
        <For each={props.agents}>
          {(agent) => {
            const statusClass = () =>
              agent.status === "ok" ? "is-active" : agent.status === "warn" ? "is-warn" : "";
            const roleColor = () => ROLE_COLORS[agent.role] ?? "var(--color-text-dim)";
            return (
              <button
                type="button"
                class={`colony-card ${statusClass()}`}
                onClick={() => props.onSelect?.(agent.id)}
              >
                <div
                  class={`pixel-sprite ${ROLE_SPRITES[agent.role] ?? "sprite-worker"}`}
                  style={{ transform: "scale(2)" }}
                />
                <div class="colony-text-3xs font-bold text-text-primary truncate w-full">
                  {agent.name}
                </div>
                <div
                  class="colony-tinted colony-text-xs uppercase"
                  style={{ "--colony-color": roleColor() }}
                >
                  {agent.role}
                </div>
                <div class="colony-text-xs text-text-dim truncate w-full">
                  {agent.connectorId ?? "roaming"}
                </div>
                {/* Fuel bar */}
                <div class="mt-auto w-full">
                  <div class="colony-stat-bar h-[3px]">
                    <div
                      class="colony-stat-fill"
                      style={{
                        width: `${agent.fuel}%`,
                        "--colony-bg":
                          agent.fuelStatus === "ok"
                            ? "var(--color-status-ok)"
                            : agent.fuelStatus === "warn"
                              ? "var(--color-status-warn)"
                              : "var(--color-status-error)",
                      }}
                    />
                  </div>
                </div>
              </button>
            );
          }}
        </For>
        {/* + New Bot card */}
        <Show when={props.onCreateNew}>
          <button
            type="button"
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
