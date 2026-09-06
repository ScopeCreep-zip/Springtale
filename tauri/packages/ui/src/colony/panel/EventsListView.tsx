import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import type { EventItem } from "../../dashboard/model";

export const EventsListView: Component<{
  events: EventItem[];
  filterConnector?: string;
}> = (props) => {
  const filtered = () => {
    if (props.filterConnector) {
      return props.events.filter((e) => e.connectorName === props.filterConnector);
    }
    return props.events;
  };

  return (
    <div>
      <div class="colony-label mb-1">
        {props.filterConnector ? `EVENTS: ${props.filterConnector}` : "EVENTS"} ({filtered().length}
        )
      </div>
      <Show
        when={filtered().length > 0}
        fallback={
          <p class="colony-text-xs py-2 text-text-dim">
            {props.filterConnector ? `No events for ${props.filterConnector}.` : "No events yet."}
          </p>
        }
      >
        <div class="space-y-0.5">
          <For each={filtered()}>
            {(event) => (
              <div class="flex items-center gap-2 border-b border-bark py-1">
                <span class="colony-text-3xs shrink-0 text-text-dim">
                  {new Date(event.timestamp).toLocaleTimeString([], {
                    hour: "2-digit",
                    minute: "2-digit",
                    second: "2-digit",
                  })}
                </span>
                <span
                  class={`colony-text-2xs shrink-0 font-bold ${
                    event.severity === "error" ? "text-status-warn" : "text-status-ok"
                  }`}
                >
                  {event.connectorName}
                </span>
                <span class="colony-text-2xs truncate text-text-secondary">
                  {event.triggerType}
                </span>
                <span class="colony-text-3xs ml-auto truncate text-text-dim">
                  {event.actionTaken}
                </span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};
