/**
 * The alert stack (ALIGNMENT-PLAN 3.6, findings 55/61/70).
 *
 * RimWorld's alert sidebar, not a toast reel: an entry exists while its
 * condition holds, and it is the way to the subject. Nothing here is on a
 * wall-clock timer — every entry is derived from live state each render, so
 * an entry disappears the moment the thing it is about stops being true.
 * The user can also dismiss one; a dismissal is remembered only while that
 * same condition holds, so a recurrence alerts again.
 *
 * The five conditions the plan names:
 *   approval required  — `db.pendingApprovals()`, answerable in place
 *   cascade hit        — newest `cascade_hit`, recent on the colony timeline
 *   sentinel quarantine— newest event for a connector reports a quarantine
 *   member marked down — newest liveness event for an agent is `down`
 *   utterance `failed` — a `failed` utterance still unexpired on the tick clock
 *
 * Severity → palette (colony.css tokens): error / warn / ok.
 */

import type { Component } from "solid-js";
import { createMemo, createSignal, For, Show } from "solid-js";
import { useDashboard } from "../dashboard/context";
import type { ColonySelection } from "./types";

export interface EventRibbonProps {
  /** Jump: select the subject and scroll the viewport to it. */
  onJump: (selection: ColonySelection) => void;
}

/** One live alert. `key` is stable for as long as the condition holds. */
interface Alert {
  key: string;
  severity: "error" | "warn" | "ok";
  title: string;
  detail: string;
  /** What the jump selects; `null` when the subject is not on the canvas. */
  target: ColonySelection | null;
  /** Present only on approval alerts — answered from the stack itself. */
  approvalId?: string;
}

/**
 * A cascade counts as live for this long measured on the colony's own event
 * timeline (newest envelope timestamp), never wall-clock — the same rule
 * `mappers.ts` uses for the ring glow.
 */
const CASCADE_RECENT_MS = 4000;

/** The one quarantine test, matching `dispatch.rs`'s `Verdict::Quarantine` text. */
function isQuarantine(actionTaken: string): boolean {
  return actionTaken.toLowerCase().includes("quarantin");
}

export const EventRibbon: Component<EventRibbonProps> = (props) => {
  const db = useDashboard();
  const [dismissed, setDismissed] = createSignal<Set<string>>(new Set());

  const alerts = createMemo<Alert[]>(() => {
    const out: Alert[] = [];
    const agentToConnector = db.agentToConnector();

    // 1. Approvals — the condition is "this approval is still pending".
    for (const a of db.pendingApprovals()) {
      const cap = typeof a.capability === "string" ? a.capability : "ACTION";
      out.push({
        key: `approval:${a.id}`,
        severity: "warn",
        title: `APPROVAL ${cap.toUpperCase()}`,
        detail: a.summary,
        target: { id: a.connector_name, type: "connector" },
        approvalId: a.id,
      });
    }

    // Cooperation events are newest-first; the colony's "now" is the newest
    // envelope's timestamp.
    const envelopes = db.cooperationEvents();
    let timelineNow = 0;
    for (const env of envelopes) {
      const ts = Date.parse(env.at);
      if (!Number.isNaN(ts) && ts > timelineNow) timelineNow = ts;
    }

    // 2. Cascades + 4. members down — first (newest) match per subject wins.
    const seenCascade = new Set<string>();
    const seenAgentLiveness = new Set<string>();
    for (const env of envelopes) {
      const ev = env.event;
      if (ev.kind === "cascade_hit" && !seenCascade.has(ev.formation_id)) {
        seenCascade.add(ev.formation_id);
        const ts = Date.parse(env.at);
        const live = !Number.isNaN(ts) && timelineNow - ts <= CASCADE_RECENT_MS;
        if (live) {
          out.push({
            key: `cascade:${ev.formation_id}`,
            severity: "ok",
            title: `CASCADE ×${ev.streak}`,
            detail: `${ev.members_affected} member(s) carried`,
            target: { id: ev.formation_id, type: "formation" },
          });
        }
      }
      if (
        (ev.kind === "member_marked_down" || ev.kind === "recovery_action_taken") &&
        "agent" in ev &&
        typeof ev.agent === "string" &&
        !seenAgentLiveness.has(ev.agent)
      ) {
        seenAgentLiveness.add(ev.agent);
        // A recovery for the same agent is newer, so it clears the alert.
        if (ev.kind === "member_marked_down") {
          const connector = agentToConnector[ev.agent];
          out.push({
            key: `down:${ev.agent}`,
            severity: "warn",
            title: "MEMBER DOWN",
            detail: `agent ${ev.agent.slice(0, 8)} since tick ${ev.since_tick}`,
            target: connector
              ? { id: connector, type: "connector" }
              : { id: ev.formation_id, type: "formation" },
          });
        }
      }
    }

    // 3. Sentinel quarantine — the condition is "the newest thing this
    //    connector did was get quarantined". A later successful action for
    //    the same connector clears it on its own.
    const seenConnector = new Set<string>();
    for (const e of db.events()) {
      if (seenConnector.has(e.connectorName)) continue;
      seenConnector.add(e.connectorName);
      if (!isQuarantine(e.actionTaken)) continue;
      out.push({
        key: `quarantine:${e.connectorName}:${e.id}`,
        severity: "error",
        title: "QUARANTINED",
        detail: `${e.connectorName}: ${e.actionTaken}`,
        target: { id: e.connectorName, type: "connector" },
      });
    }

    // 5. `failed` utterances — live while unexpired on the tick clock.
    const now = db.colonyNow();
    for (const u of db.utterances()) {
      if (u.utterance.utter !== "failed") continue;
      if (u.seq + u.ttl_ticks <= now) continue;
      const target: ColonySelection | null = u.rule_id
        ? { id: u.rule_id, type: "agent" }
        : u.agent && agentToConnector[u.agent]
          ? { id: agentToConnector[u.agent] ?? null, type: "connector" }
          : u.formation_id
            ? { id: u.formation_id, type: "formation" }
            : null;
      out.push({
        key: `failed:${u.rule_id ?? u.agent ?? u.formation_id}:${u.seq}`,
        severity: "error",
        title: "RULE FAILED",
        detail: u.label_key,
        target,
      });
    }

    return out;
  });

  /** Dismissals only survive while their condition does. */
  const visible = createMemo(() => {
    const live = alerts();
    const keys = new Set(live.map((a) => a.key));
    const gone = [...dismissed()].filter((k) => !keys.has(k));
    if (gone.length > 0) {
      setDismissed((prev) => {
        const next = new Set(prev);
        for (const k of gone) next.delete(k);
        return next;
      });
    }
    const hidden = dismissed();
    // Errors first, then warnings, then the good news; at most eight.
    const rank = { error: 0, warn: 1, ok: 2 };
    return live
      .filter((a) => !hidden.has(a.key))
      .sort((a, b) => rank[a.severity] - rank[b.severity])
      .slice(0, 8);
  });

  const dismiss = (key: string) =>
    setDismissed((prev) => {
      const next = new Set(prev);
      next.add(key);
      return next;
    });

  return (
    <Show when={visible().length > 0}>
      <output
        class="colony-alert-stack absolute left-1/2 top-1.5 z-[20] flex w-[480px] -translate-x-1/2 flex-col gap-1"
        aria-live="polite"
        aria-label="Colony alerts"
      >
        <For each={visible()}>
          {(alert) => (
            <div class="colony-event-toast" data-severity={alert.severity}>
              <button
                type="button"
                class="colony-event-toast-jump"
                disabled={!alert.target}
                onClick={() => {
                  if (alert.target) props.onJump(alert.target);
                }}
              >
                <span class="colony-event-toast-title">{alert.title}</span>
                <Show when={alert.detail}>
                  <span class="colony-event-toast-detail">{alert.detail}</span>
                </Show>
              </button>
              <Show when={alert.approvalId}>
                {(id) => (
                  <>
                    <button
                      type="button"
                      class="colony-event-toast-action"
                      data-tone="ok"
                      onClick={() => void db.resolveApproval(id(), true)}
                    >
                      APPROVE
                    </button>
                    <button
                      type="button"
                      class="colony-event-toast-action"
                      data-tone="deny"
                      onClick={() => void db.resolveApproval(id(), false)}
                    >
                      DENY
                    </button>
                  </>
                )}
              </Show>
              <button
                type="button"
                class="colony-event-toast-dismiss"
                aria-label={`Dismiss ${alert.title}`}
                onClick={() => dismiss(alert.key)}
              >
                ✕
              </button>
            </div>
          )}
        </For>
      </output>
    </Show>
  );
};
