import type { Component } from "solid-js";
import { Show } from "solid-js";
import { Canvas } from "../../Canvas";
import { useDashboard } from "../../dashboard/context";

/** Renders the bot's A2UI canvas blocks. State comes from
 *  `useDashboard().canvasState()` which subscribes to backend updates
 *  via the provider's `subscribeToCanvasUpdates`. */
export const CanvasOutputView: Component = () => {
  const db = useDashboard();
  return (
    <div class="px-4 py-3">
      <p class="colony-text-md font-bold text-text-primary">🖼 OUTPUT</p>
      <p class="colony-text-2xs mt-1 text-text-secondary">
        Structured output from the bot. Updated live as bots push new blocks.
      </p>
      <div class="mt-3">
        <Show
          when={db.canvasState()}
          fallback={
            <p class="colony-text-3xs text-text-dim">
              Nothing on the canvas yet. Once a deployed bot produces structured output, it'll
              appear here.
            </p>
          }
        >
          <Canvas state={db.canvasState()} />
        </Show>
      </div>
    </div>
  );
};
