/**
 * Typed IPC wrappers for connector operations.
 *
 * Per ARCHITECTURE.md §9: "ipc/ module: typed invoke() wrappers only —
 * no raw tauri.invoke strings."
 */
import { invoke } from "@tauri-apps/api/core";

export interface ConnectorInfo {
  name: string;
  enabled: boolean;
}

export async function listConnectors(): Promise<ConnectorInfo[]> {
  return invoke<ConnectorInfo[]>("list_connectors");
}

export async function setupConnector(
  name: string,
  config: Record<string, unknown>,
): Promise<string> {
  return invoke("setup_connector", { name, config });
}

export async function enableConnector(name: string): Promise<void> {
  return invoke("enable_connector", { name });
}

export async function disableConnector(name: string): Promise<void> {
  return invoke("disable_connector", { name });
}

/** G4 — hot-reload a connector. Thin IPC pass-through to the runtime op. */
export async function reloadConnector(name: string): Promise<void> {
  return invoke("reload_connector", { name });
}

import type { AvailableConnector } from "@springtale/types";

export type { AvailableConnector };

export async function listAvailableConnectors(): Promise<AvailableConnector[]> {
  return invoke("list_available_connectors");
}

export async function removeConnector(name: string): Promise<void> {
  return invoke("remove_connector", { name });
}

export async function removeConnectorCascade(name: string): Promise<string[]> {
  return invoke("remove_connector_cascade", { name });
}

export async function getConnectorConfig(name: string): Promise<unknown> {
  return invoke("get_connector_config", { name });
}

import type { ConnectorOutput } from "@springtale/ui";

export async function listConnectorOutputs(
  name: string,
  limit?: number,
): Promise<ConnectorOutput[]> {
  return invoke("list_connector_outputs", { name, limit: limit ?? 20 });
}

import type { ConnectorSchema } from "@springtale/types";

export async function getConnectorSchemas(): Promise<ConnectorSchema[]> {
  return invoke<ConnectorSchema[]>("get_connector_schemas");
}

/** Install a WASM connector from its manifest. */
export async function installConnector(manifest: unknown): Promise<string> {
  return invoke("install_connector", { manifest });
}
