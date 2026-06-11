/**
 * F5 — Member-picker overlay for the RM MBR command.
 *
 * Per `docs/guide/colony-canvas.md §3` (RM MBR button) + §9 (destructive
 * actions show a confirm dialog) + the existing ColonyShell overlay pattern
 * (vault, hatch wizard, settings).
 *
 * Renders the backend-supplied eligible-removal list (B11
 * `formation_eligible_members`). The frontend just renders — eligibility
 * (e.g. "cannot remove last member, use DISSOLVE instead") is decided
 * server-side per the thin-frontend rule.
 */

import type { Component } from "solid-js";
import { createResource, createSignal, For, Show } from "solid-js";
import { useDashboard } from "../dashboard/context";
import type { MemberRef } from "../dashboard/types";

export interface MemberPickerOverlayProps {
  formationId: string;
  /** Called after a member is removed — parent can refresh state / close. */
  onRemoved?: (member: MemberRef) => void;
  /** Called when the user cancels without picking. */
  onCancel: () => void;
}

export const MemberPickerOverlay: Component<MemberPickerOverlayProps> = (props) => {
  const db = useDashboard();
  const [members] = createResource(
    () => props.formationId,
    (id) => db.provider.formationEligibleMembers(id),
  );
  const [pending, setPending] = createSignal<MemberRef | null>(null);
  const [working, setWorking] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const confirmRemoval = async () => {
    const member = pending();
    if (!member) return;
    setWorking(true);
    setError(null);
    try {
      await db.provider.removeFormationMember(props.formationId, member.connector_name);
      props.onRemoved?.(member);
      props.onCancel();
    } catch (e) {
      setError(String(e));
    } finally {
      setWorking(false);
    }
  };

  return (
    <div class="mx-auto max-w-2xl rounded border-2 border-bark bg-soil-mid p-6">
      <p class="colony-text-md font-bold text-text-primary">Remove Member</p>
      <p class="colony-text-xs mt-1 text-text-secondary">
        Select an agent to remove from this formation. Removed agents are released from the
        formation but their bot remains intact.
      </p>

      <Show when={members.loading}>
        <p class="colony-text-xs mt-4 text-text-dim">Loading members…</p>
      </Show>

      <Show when={members() && members()?.length === 0}>
        <p class="colony-text-xs mt-4 text-text-dim">No members.</p>
      </Show>

      <Show when={(members()?.length ?? 0) > 0}>
        <ul class="mt-4 max-h-[40vh] overflow-y-auto">
          <For each={members()}>
            {(m) => (
              <li>
                <button
                  type="button"
                  class="w-full text-left rounded border border-bark p-2 mb-1 hover:bg-soil-deep"
                  classList={{ "is-disabled": !m.can_remove }}
                  disabled={!m.can_remove}
                  title={m.block_reason ?? "Click to remove"}
                  onClick={() => setPending(m)}
                >
                  <div class="colony-text-sm text-text-primary">
                    {m.connector_name}
                    <span class="colony-text-3xs ml-2 text-text-dim">({m.role})</span>
                  </div>
                  <div class="colony-text-3xs text-text-dim">agent: {m.agent_id}</div>
                  <Show when={!m.can_remove}>
                    <div class="colony-text-3xs text-status-warn">{m.block_reason}</div>
                  </Show>
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <Show when={error()}>
        <p class="colony-text-xs mt-2 text-status-error">{error()}</p>
      </Show>

      <Show when={pending()}>
        {/* §9 confirm-dialog pattern for destructive actions */}
        <div class="mt-4 rounded border border-status-error bg-soil-deep p-3">
          <p class="colony-text-sm text-text-primary">
            Remove <strong>{pending()?.connector_name}</strong> from this formation?
          </p>
          <div class="mt-3 flex gap-2">
            <button
              type="button"
              class="colony-command-btn colony-text-2xs px-4 py-2"
              style={{ "border-color": "var(--color-status-error)" }}
              disabled={working()}
              onClick={confirmRemoval}
            >
              {working() ? "Removing…" : "Confirm"}
            </button>
            <button
              type="button"
              class="colony-command-btn colony-text-2xs px-4 py-2"
              disabled={working()}
              onClick={() => setPending(null)}
            >
              Back
            </button>
          </div>
        </div>
      </Show>

      <div class="mt-4 flex justify-end">
        <button
          type="button"
          class="colony-command-btn colony-text-2xs px-4 py-2"
          onClick={props.onCancel}
        >
          Cancel
        </button>
      </div>
    </div>
  );
};
