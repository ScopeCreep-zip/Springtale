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

export async function enableConnector(name: string): Promise<void> {
  return invoke("enable_connector", { name });
}

export async function disableConnector(name: string): Promise<void> {
  return invoke("disable_connector", { name });
}

import type { ConnectorSchema } from "@springtale/types";

export async function getConnectorSchemas(): Promise<ConnectorSchema[]> {
  return invoke<ConnectorSchema[]>("get_connector_schemas");
}
