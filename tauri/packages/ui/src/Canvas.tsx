import type { CanvasBlock, CanvasState, StatusState } from "@springtale/types";
import type { Component } from "solid-js";
import { For, Match, Show, Switch } from "solid-js";

/**
 * Canvas — generic structured-block renderer for the A2UI surface
 * the bot pushes content to (per `docs/intended-arch/ARCHITECTURE.md`).
 *
 * Renders the four `CanvasBlock` variants — Text, Table, KeyValue,
 * Status — into accessible HTML. Never renders raw HTML strings;
 * `Text.content` is rendered as text via SolidJS auto-escape.
 *
 * This is a different surface from `ColonyCanvas` (which visualises
 * the cooperation topology). The Canvas component renders *bot
 * output*; ColonyCanvas renders *bot infrastructure*. Both can
 * coexist — desktop's `CanvasPage` uses Canvas, the colony viewport
 * uses ColonyCanvas, and a future split-pane could embed both.
 */
export interface CanvasProps {
  state: CanvasState | null;
}

const STATUS_BORDER: Record<StatusState, string> = {
  Info: "border-blue-500/40",
  Success: "border-green-500/40",
  Warning: "border-yellow-500/40",
  Error: "border-red-500/40",
  Loading: "border-gray-500/40",
};

const STATUS_TEXT: Record<StatusState, string> = {
  Info: "text-blue-300",
  Success: "text-green-300",
  Warning: "text-yellow-300",
  Error: "text-red-300",
  Loading: "text-gray-300",
};

export const Canvas: Component<CanvasProps> = (props) => {
  return (
    <Show
      when={props.state && props.state.blocks.length > 0}
      fallback={
        <p class="text-sm text-gray-400 italic">
          No content. The bot will push blocks here when it has something to show.
        </p>
      }
    >
      <div class="space-y-3">
        <For each={props.state?.blocks}>{(block) => <CanvasBlockView block={block} />}</For>
      </div>
    </Show>
  );
};

const CanvasBlockView: Component<{ block: CanvasBlock }> = (props) => (
  <Switch>
    <Match when={props.block.type === "Text"}>
      {(() => {
        const b = props.block as Extract<CanvasBlock, { type: "Text" }>;
        return (
          <div class="rounded border border-gray-700 bg-gray-800/50 p-3 text-sm text-gray-200">
            {b.content}
          </div>
        );
      })()}
    </Match>
    <Match when={props.block.type === "Table"}>
      {(() => {
        const b = props.block as Extract<CanvasBlock, { type: "Table" }>;
        return (
          <table class="w-full border-collapse text-sm">
            <thead>
              <tr class="border-b border-gray-700">
                <For each={b.headers}>
                  {(header) => (
                    <th class="px-3 py-2 text-left font-medium text-gray-300">{header}</th>
                  )}
                </For>
              </tr>
            </thead>
            <tbody>
              <For each={b.rows}>
                {(row) => (
                  <tr class="border-b border-gray-800">
                    <For each={row}>
                      {(cell) => <td class="px-3 py-2 text-gray-200">{cell}</td>}
                    </For>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        );
      })()}
    </Match>
    <Match when={props.block.type === "KeyValue"}>
      {(() => {
        const b = props.block as Extract<CanvasBlock, { type: "KeyValue" }>;
        return (
          <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
            <For each={b.pairs}>
              {([key, value]) => (
                <>
                  <dt class="font-medium text-gray-400">{key}</dt>
                  <dd class="text-gray-200">{value}</dd>
                </>
              )}
            </For>
          </dl>
        );
      })()}
    </Match>
    <Match when={props.block.type === "Status"}>
      {(() => {
        const b = props.block as Extract<CanvasBlock, { type: "Status" }>;
        return (
          <div
            class={`rounded border-l-4 bg-gray-800/50 px-3 py-2 ${STATUS_BORDER[b.state]}`}
            role={b.state === "Error" || b.state === "Warning" ? "alert" : "status"}
            aria-live={b.state === "Error" ? "assertive" : "polite"}
          >
            <p class={`text-sm font-medium ${STATUS_TEXT[b.state]}`}>{b.label}</p>
            <Show when={b.message}>
              <p class="mt-1 text-xs text-gray-400">{b.message}</p>
            </Show>
          </div>
        );
      })()}
    </Match>
  </Switch>
);
