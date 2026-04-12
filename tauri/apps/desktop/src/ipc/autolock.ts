import { invoke } from "@tauri-apps/api/core";

/** Reset the auto-lock inactivity timer. */
export function resetAutoLock(): Promise<void> {
  return invoke("reset_auto_lock");
}
