/**
 * W1.F — IPC wrapper for the approval-gate response command.
 *
 * The destructive-action approval event itself arrives on the Tauri
 * `approval-required` listener (set up in App.tsx). This module only
 * exposes the response side.
 */
import { invoke } from "@tauri-apps/api/core";

export interface ApprovalEventPayload {
  request_id: string;
  connector_name: string;
  action_type: string;
  rationale: string;
}

export async function respondToApproval(
  requestId: string,
  approve: boolean,
): Promise<void> {
  return invoke("respond_to_approval", { requestId, approve });
}
