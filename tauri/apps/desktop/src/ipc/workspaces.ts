/**
 * Typed IPC wrappers for the external-workspace directory (D1).
 *
 * Sizes-only metadata flows through these — `display_name`,
 * `kind`, `metadata_json` (a string-serialized JSON blob),
 * `provenance_json`. No message bodies, no roster lists past a
 * count.
 */

import type { ChatDiscoveredEvent, WorkspaceInfo } from "@springtale/ui/dashboard/types";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export async function listWorkspaces(
  formationId: string,
  connectorFilter?: string,
): Promise<WorkspaceInfo[]> {
  return invoke<WorkspaceInfo[]>("list_workspaces", {
    formationId,
    connectorFilter: connectorFilter ?? null,
  });
}

export async function scanWorkspaces(
  formationId: string,
  connectorName: string,
): Promise<WorkspaceInfo[]> {
  return invoke<WorkspaceInfo[]>("scan_workspaces", {
    formationId,
    connectorName,
  });
}

export async function deleteWorkspace(formationId: string, workspaceKey: string): Promise<void> {
  return invoke<void>("delete_workspace", { formationId, workspaceKey });
}

export async function upsertWorkspaceManual(
  formationId: string,
  workspaceKey: string,
  displayName: string,
  connectorName: string,
  kind: string,
): Promise<void> {
  return invoke<void>("upsert_workspace_manual_cmd", {
    formationId,
    workspaceKey,
    displayName,
    connectorName,
    kind,
  });
}

export async function previewOnboardUrl(
  connectorName: string,
  config: Record<string, unknown>,
  payload?: string,
): Promise<string> {
  return invoke<string>("preview_onboard_url", {
    connectorName,
    config,
    payload: payload ?? null,
  });
}

/**
 * Track D — kick off the 60s auto-onboard stream.
 *
 * The backend spawns a tokio task that polls the connector's
 * `discover_destinations` action every 2 seconds, emitting a
 * `chat-discovered` event for every match. Pair with
 * `subscribeToChatDiscovered` (started BEFORE this call so the
 * listener doesn't miss the first event).
 */
export async function startOnboardStream(
  sessionId: string,
  connectorName: string,
  config: Record<string, unknown>,
  payload?: string,
): Promise<void> {
  return invoke<void>("start_onboard_stream", {
    sessionId,
    connectorName,
    config,
    payload: payload ?? null,
  });
}

/**
 * Tear down an active onboard stream. Idempotent — a vacant
 * session id is a no-op. Pairs with `onCleanup` in the picker.
 */
export async function cancelOnboardStream(sessionId: string): Promise<void> {
  return invoke<void>("cancel_onboard_stream", { sessionId });
}

/**
 * Subscribe to `chat-discovered` events. Returns the unlisten
 * function — call it from `onCleanup` to stop listening. The
 * caller filters by `session_id` so multiple concurrent picker
 * mounts stay isolated.
 */
export async function subscribeToChatDiscovered(
  callback: (event: ChatDiscoveredEvent) => void,
): Promise<UnlistenFn> {
  return listen<ChatDiscoveredEvent>("chat-discovered", (e) => callback(e.payload));
}
