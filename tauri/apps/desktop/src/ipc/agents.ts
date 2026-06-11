/**
 * Typed IPC wrappers for agent operations.
 */

import type { AgentState } from "@springtale/types";
import { invoke } from "@tauri-apps/api/core";

export async function listAgentStates(): Promise<AgentState[]> {
  return invoke<AgentState[]>("list_agent_states");
}

export async function getAutonomy(name: string): Promise<string> {
  return invoke("get_autonomy", { name });
}

export async function setAutonomy(name: string, level: string): Promise<void> {
  return invoke("set_autonomy", { name, level });
}

export async function stepAutonomy(name: string, direction: "up" | "down"): Promise<string> {
  return invoke<string>("step_autonomy", { name, direction });
}
