import { invoke } from "@tauri-apps/api/core";

/**
 * Reset the auto-lock inactivity timer.
 *
 * The shell no longer reads the safety table, so the interval comes
 * from the caller — the value it fetched with `GET /safety`.
 */
export function resetAutoLock(autoLockMinutes: number): Promise<void> {
  return invoke("reset_auto_lock", { autoLockMinutes });
}
