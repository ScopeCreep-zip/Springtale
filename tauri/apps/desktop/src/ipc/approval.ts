/**
 * W1.F — IPC wrapper for the approval-gate response command.
 *
 * The destructive-action approval event itself arrives on the Tauri
 * `approval-required` listener (set up in App.tsx). This module only
 * exposes the response side.
 */
import type { ApprovalInfo } from "@springtale/ui";
import { invoke } from "@tauri-apps/api/core";

export interface ApprovalEventPayload {
  request_id: string;
  connector_name: string;
  action_type: string;
  rationale: string;
}

export async function respondToApproval(requestId: string, approve: boolean): Promise<void> {
  return invoke("respond_to_approval", { requestId, approve });
}

/**
 * Plan 6.7 — the runtime chat gate's pending queue. Same shape as the
 * daemon's `GET /approvals`; a different queue from the sentinel
 * dispatcher above (see `commands/approval.rs`).
 */
export async function listPendingApprovals(): Promise<ApprovalInfo[]> {
  const r = await invoke<{ pending: ApprovalInfo[] }>("list_pending_approvals");
  return r.pending;
}

/** Plan 6.7 — land a decision on the runtime chat gate's queue. */
export async function resolveApproval(requestId: string, approve: boolean): Promise<void> {
  return invoke("resolve_approval", { requestId, approve, reason: null });
}
