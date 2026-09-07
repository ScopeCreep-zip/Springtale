/**
 * Sidecar lifecycle events.
 *
 * The desktop shell is a client of `springtaled`: unlocking the vault
 * spawns the daemon and every read and write goes to its loopback API.
 * If that process dies the window is left holding a port and a token
 * that lead nowhere, so Rust supervises the child (`sidecar::supervise`)
 * and emits `daemon-stopped` when it goes away unexpectedly. A vault
 * lock, auto-lock or quick-hide stops the daemon deliberately and does
 * NOT emit this — receiving it always means something went wrong.
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Payload of the Rust `DaemonStopped` event. */
export interface DaemonStopped {
  /** Process exit code, when the platform reported one. */
  code: number | null;
}

/** Subscribe to unexpected daemon termination. */
export async function onDaemonStopped(
  handler: (payload: DaemonStopped) => void,
): Promise<UnlistenFn> {
  return listen<DaemonStopped>("daemon-stopped", (event) => handler(event.payload));
}
