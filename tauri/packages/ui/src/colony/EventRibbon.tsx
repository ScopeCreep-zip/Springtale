/**
 * Phase H6 — EventRibbon: high-severity cooperation event toasts.
 *
 * Renders ephemeral 4s toasts at the top of the colony viewport for the
 * subset of cooperation events the user must see now: interventions
 * firing, supervisor escalations, members marked down, voluntary
 * sacrifices. All other variants live in the BottomPanel formation log.
 *
 * Severity → palette (colony.css tokens):
 *   intervention_fired / supervisor_escalated → status-error
 *   member_marked_down                        → status-warn
 *   sacrifice_yield                           → status-ok (positive cooperation)
 *
 * The component subscribes to `db.cooperationEvents()` and tracks the
 * highest seq it has surfaced so re-renders after history backfill never
 * re-toast old envelopes.
 */

import type { Component } from "solid-js";
import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { useDashboard } from "../dashboard/context";
import type { CooperationEvent, CooperationEventEnvelope } from "../dashboard/types";

interface Toast {
  seq: number;
  at: string;
  kind: CooperationEvent["kind"];
  severity: "error" | "warn" | "ok";
  title: string;
  detail: string;
  timer: ReturnType<typeof setTimeout>;
}

const TOAST_TTL_MS = 4000;
const HIGH_SEVERITY_KINDS: ReadonlySet<CooperationEvent["kind"]> = new Set([
  "intervention_fired",
  "supervisor_escalated",
  "member_marked_down",
  "sacrifice_yield",
]);

function severityFor(kind: CooperationEvent["kind"]): "error" | "warn" | "ok" {
  switch (kind) {
    case "intervention_fired":
    case "supervisor_escalated":
      return "error";
    case "member_marked_down":
      return "warn";
    case "sacrifice_yield":
      return "ok";
    default:
      return "warn";
  }
}

function describe(event: CooperationEvent): { title: string; detail: string } {
  switch (event.kind) {
    case "intervention_fired":
      return {
        title: `INTERVENTION ${event.intervention.intervention.toUpperCase()}`,
        detail: event.summary,
      };
    case "supervisor_escalated":
      return { title: "ESCALATED", detail: event.reason };
    case "member_marked_down":
      return {
        title: "MEMBER DOWN",
        detail: `agent ${event.agent.slice(0, 8)} since tick ${event.since_tick}`,
      };
    case "sacrifice_yield":
      return {
        title: "SACRIFICE YIELD",
        detail: `${event.sacrificer.slice(0, 8)} → ${event.beneficiary.slice(0, 8)} (utility ${event.utility.toFixed(2)})`,
      };
    default:
      return { title: event.kind.toUpperCase(), detail: "" };
  }
}

export const EventRibbon: Component = () => {
  const db = useDashboard();
  const [toasts, setToasts] = createSignal<Toast[]>([]);
  // Track the highest seq we've already surfaced so backfill / re-render
  // doesn't re-toast old envelopes. Starts at the highest seq currently in
  // the ring so the user doesn't get a wall of toasts on first mount.
  const initial = db.cooperationEvents();
  let lastSurfacedSeq = initial[0]?.seq ?? -1;

  const dismiss = (seq: number) => {
    setToasts((prev) => {
      const found = prev.find((t) => t.seq === seq);
      if (found) clearTimeout(found.timer);
      return prev.filter((t) => t.seq !== seq);
    });
  };

  createEffect(() => {
    const envelopes = db.cooperationEvents();
    // Envelopes are most-recent-first; walk newest → oldest, stop when we
    // hit something we've already surfaced.
    const fresh: CooperationEventEnvelope[] = [];
    for (const env of envelopes) {
      if (env.seq <= lastSurfacedSeq) break;
      if (HIGH_SEVERITY_KINDS.has(env.event.kind)) fresh.push(env);
    }
    const top = envelopes[0];
    if (top) lastSurfacedSeq = top.seq;
    if (fresh.length === 0) return;
    // Push oldest-of-the-fresh first so visual order is newest-on-top.
    const additions: Toast[] = [];
    for (const env of fresh.slice().reverse()) {
      const { title, detail } = describe(env.event);
      const timer = setTimeout(() => dismiss(env.seq), TOAST_TTL_MS);
      additions.push({
        seq: env.seq,
        at: env.at,
        kind: env.event.kind,
        severity: severityFor(env.event.kind),
        title,
        detail,
        timer,
      });
    }
    setToasts((prev) => [...additions.reverse(), ...prev].slice(0, 6));
  });

  onCleanup(() => {
    for (const t of toasts()) clearTimeout(t.timer);
  });

  return (
    <Show when={toasts().length > 0}>
      <div class="pointer-events-none absolute left-1/2 top-1.5 z-[20] flex w-[480px] -translate-x-1/2 flex-col gap-1">
        <For each={toasts()}>
          {(toast) => (
            <div
              class="colony-event-toast pointer-events-auto"
              data-severity={toast.severity}
              role="status"
              aria-live="polite"
            >
              <span class="colony-event-toast-title">{toast.title}</span>
              <Show when={toast.detail}>
                <span class="colony-event-toast-detail">{toast.detail}</span>
              </Show>
              <button
                type="button"
                class="colony-event-toast-dismiss"
                aria-label="Dismiss"
                onClick={() => dismiss(toast.seq)}
              >
                ✕
              </button>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
};
