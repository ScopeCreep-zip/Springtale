import { invoke } from "@tauri-apps/api/core";

export interface SessionRow {
  id: string;
  user_id: string;
  channel_id: string;
  connector: string;
  created_at: string;
  last_active: string;
}

/** List all active sessions. */
export function listSessions(): Promise<SessionRow[]> {
  return invoke("list_sessions");
}
