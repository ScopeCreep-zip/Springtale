/**
 * Formation (swarm) IPC wrappers.
 *
 * Formations are the user-facing abstraction: groups of cooperating
 * agents deployed across substrates (connectors). Users think in
 * swarms, the system executes rules.
 */
import { invoke } from "@tauri-apps/api/core";

export interface FormationInfo {
  id: string;
  name: string;
  intent: string;
  status: string;
  member_count: number;
}

export async function createFormation(
  name: string,
  intent: string,
  connectors: string[],
): Promise<string> {
  return invoke<string>("create_formation", { name, intent, connectors });
}

export async function deployFormation(id: string): Promise<void> {
  return invoke("deploy_formation", { id });
}

export async function pauseFormation(id: string): Promise<void> {
  return invoke("pause_formation", { id });
}

export async function resumeFormation(id: string): Promise<void> {
  return invoke("resume_formation", { id });
}

export async function dissolveFormation(id: string): Promise<void> {
  return invoke("dissolve_formation", { id });
}

export async function listFormations(): Promise<FormationInfo[]> {
  return invoke<FormationInfo[]>("list_formations");
}
