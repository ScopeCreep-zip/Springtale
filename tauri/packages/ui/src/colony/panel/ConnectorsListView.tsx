import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import type { ColonyNode } from "../types";
import { NODE_SIZES, NODE_SPRITES, seeded } from "../types";

export const ConnectorsListView: Component<{
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
      <Show
        when={props.nodes.length > 0}
        fallback={<p class="colony-text-2xs py-1 text-text-dim">No connectors loaded yet.</p>}
      >
        <div class="colony-card-strip mb-3">
          <For each={props.nodes}>
            {(node) => {
              const spriteClass = NODE_SPRITES[node.type] ?? "sprite-tree-deciduous";
              const size = NODE_SIZES[node.type] ?? { width: 36, height: 44 };
              const statusClass = () =>
                node.status === "active" ? "is-active" : node.status === "paused" ? "is-warn" : "";
              return (
                <button
                  type="button"
                  class={`colony-card ${statusClass()}`}
                  onClick={() => props.onSelect?.(node.id)}
                >
                  <div class="colony-card-sprite-frame" style={{ width: `${size.width / 2}px` }}>
                    <div class={`pixel-sprite ${spriteClass}`} style={{ transform: "scale(2)" }} />
                  </div>
                  <div class="colony-text-3xs font-bold text-text-primary truncate w-full">
                    {node.label.replace("connector-", "")}
                  </div>
                  <div
                    class={`colony-text-xs uppercase ${
                      node.status === "active"
                        ? "text-status-ok"
                        : node.status === "paused"
                          ? "text-status-warn"
                          : "text-text-dim"
                    }`}
                  >
                    {node.status}
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
              const nodeType =
                ["conifer", "deciduous", "shrub"][seeded(`${connector.name}type`, 0, 3)] ??
                "deciduous";
              const spriteClass =
                NODE_SPRITES[nodeType as keyof typeof NODE_SPRITES] ?? "sprite-tree-deciduous";
              const size = NODE_SIZES[nodeType] ?? { width: 36, height: 44 };
              return (
                <button
                  type="button"
                  class="colony-card is-available"
                  onClick={() => props.onSetup?.(connector.name)}
                >
                  <div class="colony-card-sprite-frame" style={{ width: `${size.width / 2}px` }}>
                    <div
                      class={`pixel-sprite ${spriteClass} opacity-50`}
                      style={{ transform: "scale(2)" }}
                    />
                  </div>
                  <div class="colony-text-3xs text-text-secondary truncate w-full">
                    {connector.name.replace("connector-", "")}
                  </div>
                  <div class="colony-text-xs text-status-ok">
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
