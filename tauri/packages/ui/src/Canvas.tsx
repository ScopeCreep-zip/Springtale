import { For, Switch, Match, Show } from "solid-js";
import type { Component } from "solid-js";
import type { CanvasBlock, CanvasState, StatusState } from "@springtale/types";
import { useI18n } from "./i18n/context";

export interface CanvasProps {
  state: CanvasState | null;
}

/**
 * Canvas/A2UI — renders structured content pushed by the bot.
 *
 * Per ARCHITECTURE.md: "Canvas receives structured data (typed
 * SolidJS stores), not raw HTML. No innerHTML or
 * dangerouslySetInnerHTML."
 *
 * Each CanvasBlock variant maps to a semantic HTML element:
 * - Text → <p>
 * - Table → <table> with role="grid"
 * - KeyValue → <dl> (definition list)
 * - Status → status card with aria-live
 */
export const Canvas: Component<CanvasProps> = (props) => {
  const { t } = useI18n();

  return (
    <div>
      <Show when={props.state} fallback={
        <p role="status" class="text-gray-500">{t("canvas.empty")}</p>
      }>
        {(state) => (
          <>
            <Show when={state().title}>
              <h2 class="mb-4 text-lg font-semibold text-white">{state().title}</h2>
            </Show>
            <div class="space-y-4">
              <For each={state().blocks}>
                {(block) => <CanvasBlockRenderer block={block} />}
              </For>
            </div>
          </>
        )}
      </Show>
    </div>
  );
};

const CanvasBlockRenderer: Component<{ block: CanvasBlock }> = (props) => {
  return (
    <Switch>
      <Match when={props.block.type === "Text" && props.block}>
        {(block) => (
          <p class="text-sm text-gray-300">
            {(block() as Extract<CanvasBlock, { type: "Text" }>).content}
          </p>
        )}
      </Match>

      <Match when={props.block.type === "Table" && props.block}>
        {(block) => {
          const b = () => block() as Extract<CanvasBlock, { type: "Table" }>;
          return (
            <div class="overflow-x-auto rounded border border-gray-700">
              <table role="grid" class="w-full text-sm">
                <thead>
                  <tr class="border-b border-gray-700 bg-gray-800/50">
                    <For each={b().headers}>
                      {(header) => (
                        <th scope="col" class="px-3 py-2 text-start font-medium text-gray-300">
                          {header}
                        </th>
                      )}
                    </For>
                  </tr>
                </thead>
                <tbody>
                  <For each={b().rows}>
                    {(row) => (
                      <tr class="border-b border-gray-800">
                        <For each={row}>
                          {(cell) => (
                            <td class="px-3 py-2 text-gray-400">{cell}</td>
                          )}
                        </For>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          );
        }}
      </Match>

      <Match when={props.block.type === "KeyValue" && props.block}>
        {(block) => {
          const b = () => block() as Extract<CanvasBlock, { type: "KeyValue" }>;
          return (
            <dl class="rounded border border-gray-700 bg-gray-800/50 px-4 py-3">
              <For each={b().pairs}>
                {([key, value]) => (
                  <div class="flex justify-between py-1">
                    <dt class="font-medium text-gray-300">{key}</dt>
                    <dd class="text-gray-400">{value}</dd>
                  </div>
                )}
              </For>
            </dl>
          );
        }}
      </Match>

      <Match when={props.block.type === "Status" && props.block}>
        {(block) => {
          const b = () => block() as Extract<CanvasBlock, { type: "Status" }>;
          return (
            <div
              aria-live="polite"
              class={`rounded border px-4 py-3 ${statusColors(b().state)}`}
            >
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium">{b().label}</span>
                <StatusIndicator state={b().state} />
              </div>
              <Show when={b().message}>
                <p class="mt-1 text-sm opacity-80">{b().message}</p>
              </Show>
            </div>
          );
        }}
      </Match>
    </Switch>
  );
};

const StatusIndicator: Component<{ state: StatusState }> = (props) => {
  const dotColor = () => {
    switch (props.state) {
      case "Info": return "bg-blue-400";
      case "Success": return "bg-green-400";
      case "Warning": return "bg-yellow-400";
      case "Error": return "bg-red-400";
      case "Loading": return "bg-gray-400 motion-safe:animate-pulse";
      default: return "bg-gray-400";
    }
  };

  return <span class={`inline-block h-2 w-2 rounded-full ${dotColor()}`} aria-hidden="true" />;
};

function statusColors(state: StatusState): string {
  switch (state) {
    case "Info": return "border-blue-500/30 bg-blue-500/10 text-blue-300";
    case "Success": return "border-green-500/30 bg-green-500/10 text-green-300";
    case "Warning": return "border-yellow-500/30 bg-yellow-500/10 text-yellow-200";
    case "Error": return "border-red-500/30 bg-red-500/10 text-red-300";
    case "Loading": return "border-gray-500/30 bg-gray-500/10 text-gray-300";
    default: return "border-gray-500/30 bg-gray-500/10 text-gray-300";
  }
}
