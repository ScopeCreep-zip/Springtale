/**
 * Typed IPC wrappers for config + data operations.
 *
 * Agent autonomy moved to ipc/agents.ts.
 */
import { invoke } from "@tauri-apps/api/core";

// Config
export async function getConfig(key: string): Promise<unknown> {
  return invoke("get_config", { key });
}

export async function setConfig(key: string, value: unknown): Promise<void> {
  return invoke("set_config", { key, value });
}

export async function listConfig(): Promise<Array<[string, unknown]>> {
  return invoke("list_config");
}

export async function setAiAdapter(config: Record<string, unknown>): Promise<void> {
  return invoke("set_ai_adapter", { config });
}

export async function setConnectorConfig(name: string, config: Record<string, unknown>): Promise<void> {
  return invoke("set_connector_config", { name, config });
}

export async function configureAiAdapter(target: string, config: Record<string, unknown>): Promise<void> {
  return invoke("configure_ai_adapter", { target, config });
}

export async function upsertConnectorConfig(name: string, config: Record<string, unknown>): Promise<boolean> {
  return invoke("upsert_connector_config", { name, config });
}

export async function toggleFormationGuard(formationId: string): Promise<boolean> {
  return invoke("toggle_formation_guard", { formationId });
}

// Data
export async function exportData(): Promise<unknown> {
  return invoke("export_data");
}

// Memory
export async function auditMemory(): Promise<unknown> {
  return invoke("audit_memory");
}

export async function compactMemory(maxEntries: number): Promise<void> {
  return invoke("compact_memory", { maxEntries });
}

