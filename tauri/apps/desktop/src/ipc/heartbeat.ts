import { invoke } from "@tauri-apps/api/core";

/** Get the current heartbeat value. */
export function getHeartbeat(): Promise<unknown> {
  return invoke("get_heartbeat");
}

/** Set the heartbeat value. */
export function setHeartbeat(value: unknown): Promise<void> {
  return invoke("set_heartbeat", { value });
}
