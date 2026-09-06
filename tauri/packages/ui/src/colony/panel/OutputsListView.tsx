import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import type { ConnectorOutput } from "../../dashboard/types";

export const OutputsListView: Component<{
  outputs: ConnectorOutput[];
  connectorId: string;
}> = (props) => (
  <div>
    <div class="colony-label mb-1">
      OUTPUTS: {props.connectorId} ({props.outputs.length})
    </div>
    <Show
      when={props.outputs.length > 0}
      fallback={
        <p class="colony-text-xs py-2 text-text-dim">
          No execution results yet for {props.connectorId}.
        </p>
      }
    >
      <div class="space-y-1">
        <For each={props.outputs}>
          {(output) => (
            <div
              class={`rounded border p-2 ${output.success ? "border-bark" : "border-status-error"}`}
            >
              <div class="flex items-center gap-2">
                <span
                  class={`inline-block h-2 w-2 rounded-full ${output.success ? "bg-status-ok" : "bg-status-error"}`}
                />
                <span class="colony-text-2xs font-bold text-text-primary">
                  {output.rule_name ?? "unknown"}
                </span>
                <span class="colony-text-3xs ml-auto text-text-dim">
                  {new Date(output.created_at).toLocaleTimeString([], {
                    hour: "2-digit",
                    minute: "2-digit",
                    second: "2-digit",
                  })}
                </span>
              </div>
              <Show when={output.error_message}>
                <p class="colony-text-3xs mt-1 text-status-error">{output.error_message}</p>
              </Show>
              <pre class="colony-text-3xs mt-1 max-h-20 overflow-auto whitespace-pre-wrap text-text-secondary">
                {(() => {
                  try {
                    return JSON.stringify(JSON.parse(output.output_json), null, 2);
                  } catch {
                    return output.output_json;
                  }
                })()}
              </pre>
            </div>
          )}
        </For>
      </div>
    </Show>
  </div>
);

// ── W2.E Canvas tab — A2UI structured-output surface ──────────
