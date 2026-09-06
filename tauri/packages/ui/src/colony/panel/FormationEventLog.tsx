import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import { useDashboard } from "../../dashboard/context";
import type { CooperationEventEnvelope } from "../../dashboard/types";
import { detailFor, EVENT_LABELS, severityFor } from "./eventLabels";

export const FormationEventLog: Component<{ formationId: string }> = (props) => {
  const db = useDashboard();
  const filtered = (): CooperationEventEnvelope[] =>
    db
      .cooperationEvents()
      .filter((env) => "formation_id" in env.event && env.event.formation_id === props.formationId)
      .slice(0, 50);

  return (
    <>
      <div class="colony-label mt-1.5 mb-0.5">EVENTS ({filtered().length})</div>
      <Show
        when={filtered().length > 0}
        fallback={<p class="colony-text-3xs py-0.5 text-text-dim">No cooperation events yet.</p>}
      >
        <div class="colony-event-log">
          <For each={filtered()}>
            {(env) => (
              <div class="colony-event-log-entry" data-severity={severityFor(env.event.kind)}>
                <span class="colony-event-log-time">
                  {new Date(env.at).toLocaleTimeString([], {
                    hour: "2-digit",
                    minute: "2-digit",
                    second: "2-digit",
                  })}
                </span>
                <span class="colony-event-log-kind">{EVENT_LABELS[env.event.kind]}</span>
                <span class="colony-event-log-detail">{detailFor(env.event)}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </>
  );
};

// ── Formation AI adapter row (G7) ────────────────────────
