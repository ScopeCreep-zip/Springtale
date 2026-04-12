import { invoke } from "@tauri-apps/api/core";

/** Get bot runtime status. */
export function getBotStatus(): Promise<unknown> {
  return invoke("bot_status");
}

/** Get bot memory / session data. */
export function getBotMemory(): Promise<unknown> {
  return invoke("bot_memory");
}
