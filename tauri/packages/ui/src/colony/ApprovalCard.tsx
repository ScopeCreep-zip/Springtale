/**
 * ApprovalCard — W1.F destructive-action confirmation overlay.
 *
 * Renders when the backend emits an `approval-required` event. Two
 * big buttons: Approve / Deny. Plain-language description of the
 * action so the survivor knows what they're agreeing to.
 *
 * Pure renderer — the parent app owns the event listener + the
 * "respond" IPC call. This component only emits the user's decision
 * upward via `onDecision`.
 *
 * Pattern reference: Pokemon-style confirmation card (two clear
 * options, no destructive default), Animal Crossing crafting
 * confirmation (icon + summary + buttons).
 */

import { type Component, createSignal, onCleanup, onMount, Show } from "solid-js";

export interface ApprovalCardProps {
  /** Connector that originated the action (e.g. `connector-github`). */
  connectorName: string;
  /** Discriminant of the action (e.g. `DeleteFile`). */
  actionType: string;
  /** Plain-language rationale assembled by the backend. */
  rationale: string;
  /** ISO timestamp of the gate's deny-by-default deadline; omit when unknown. */
  expiresAt?: string | null;
  /** Fires when the user clicks Approve or Deny. */
  onDecision: (approve: boolean) => void;
}

/** `mm:ss` until `expiresAt`, `"expired"` once past, `null` when unknown. */
function countdown(expiresAt: string | null | undefined, nowMs: number): string | null {
  if (!expiresAt) return null;
  const ms = Date.parse(expiresAt) - nowMs;
  if (Number.isNaN(ms)) return null;
  if (ms <= 0) return "expired";
  const total = Math.floor(ms / 1000);
  const mm = String(Math.floor(total / 60)).padStart(2, "0");
  const ss = String(total % 60).padStart(2, "0");
  return `${mm}:${ss}`;
}

export const ApprovalCard: Component<ApprovalCardProps> = (props) => {
  // Tick once a second so the deadline reads as a live countdown.
  const [now, setNow] = createSignal(Date.now());
  onMount(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    onCleanup(() => clearInterval(timer));
  });
  const remaining = () => countdown(props.expiresAt, now());
  return (
    <div class="mx-auto max-w-lg rounded border-2 border-status-warn bg-soil-mid p-6">
      <p class="colony-text-md font-bold text-text-primary">⚠️ Destructive action</p>
      <p class="colony-text-xs mt-2 text-text-secondary">{props.rationale}</p>
      <div class="mt-3 rounded border border-bark bg-soil-deep p-3">
        <p class="colony-text-3xs text-text-dim">Action</p>
        <p class="colony-text-xs mt-1 text-text-primary">{props.actionType}</p>
        <p class="colony-text-3xs mt-2 text-text-dim">Connector</p>
        <p class="colony-text-xs mt-1 text-text-primary">
          {props.connectorName.replace(/^connector-/, "")}
        </p>
      </div>
      <Show when={remaining()}>
        {(r) => (
          <p class="colony-text-3xs mt-2 text-text-dim">
            {r() === "expired" ? "Expired — denied by default" : `Auto-deny in ${r()}`}
          </p>
        )}
      </Show>
      <p class="colony-text-3xs mt-3 text-text-dim">
        Approving will let this connector run the action once. The bot can request approval again
        for future identical actions.
      </p>
      <div class="mt-6 flex justify-end gap-2">
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-5 py-2"
          onClick={() => props.onDecision(false)}
        >
          Deny
        </button>
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-5 py-2"
          style={{ "border-color": "var(--color-status-warn)" }}
          onClick={() => props.onDecision(true)}
        >
          Approve
        </button>
      </div>
    </div>
  );
};
