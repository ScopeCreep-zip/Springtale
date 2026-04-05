/**
 * Typed IPC wrappers for event operations.
 */
import { invoke } from "@tauri-apps/api/core";
import type { EventEntry } from "@springtale/types";

export async function listEvents(limit: number = 50): Promise<EventEntry[]> {
  return invoke<EventEntry[]>("list_events", { limit });
}
