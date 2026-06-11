/**
 * Typed IPC wrappers for event operations.
 */

import type { EventEntry } from "@springtale/types";
import { invoke } from "@tauri-apps/api/core";

export async function listEvents(limit: number = 50): Promise<EventEntry[]> {
  return invoke<EventEntry[]>("list_events", { limit });
}
