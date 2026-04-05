/**
 * Panic wipe IPC wrapper — emergency data destruction.
 *
 * Per ARCHITECTURE.md §2.6: must complete within 3 seconds.
 * No parameters — speed is critical in an emergency.
 */
import { invoke } from "@tauri-apps/api/core";

export async function panicWipe(): Promise<void> {
  return invoke("panic_wipe");
}
