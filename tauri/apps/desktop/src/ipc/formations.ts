/**
 * Formation (swarm) IPC wrappers.
 *
 * Formations are the user-facing abstraction: groups of cooperating
 * agents deployed across substrates (connectors). Users think in
 * swarms, the system executes rules.
 */
import { invoke } from "@tauri-apps/api/core";

/** Structured agent health — tagged union matching backend
 * `AgentHealthDetail`. Replaces the old stringified Debug output. */
export type AgentHealthDetail =
  | { type: "Operational" }
  | { type: "Degraded"; recovery_count: number }
  | { type: "Incapacitated" }
  | { type: "Dead"; recoverable: boolean };

export interface FormationInfo {
  id: string;
  name: string;
  intent: string;
  status: string;
  member_count: number;
  /** Members whose health is Operational or Degraded — drives the
   *  "X/Y operational" aggregate-stats row. */
  operational_count: number;
  members: string[];
  /** Real momentum tier from backend: "Cold", "Warming", "Hot", "Fever". */
  momentum_tier: string;
  /** Human label for momentum tier: "COLD", "WARM", "HOT", "FEVER". */
  momentum_label: string;
  /** Consecutive successful ticks in the current run. */
  momentum_consecutive_successes: number;
  /** Interference count accumulated in the current tier. */
  momentum_interference_count: number;
  /** Successes remaining to promote to the next tier. null at Fever. */
  momentum_successes_to_next_tier: number | null;
  /** Capabilities unlocked at current tier. */
  capabilities: string[];
  /** Guard readiness: "OK" if any member active, "--" otherwise. */
  guard_status: string;
  /** Rally tokens remaining (Monster Hunter carts, section 15). */
  rally_tokens: number;
  /** Maximum rally tokens. */
  rally_max: number;
}

export interface FormationMemberDetail {
  agent_id: string;
  connector_name: string;
  role: string;
  health: AgentHealthDetail;
  fuel_remaining: number;
  liveness: string;
  attention_load: number;
  active_task: string | null;
  consecutive_failures: number;
}

export interface FormationDetail extends FormationInfo {
  member_details: FormationMemberDetail[];
}

export async function getFormation(id: string): Promise<FormationDetail> {
  return invoke<FormationDetail>("get_formation", { id });
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

export async function rallyFormation(id: string): Promise<void> {
  return invoke("rally_formation", { id });
}

export async function listFormations(): Promise<FormationInfo[]> {
  return invoke<FormationInfo[]>("list_formations");
}

export async function updateFormationIntent(id: string, intent: string): Promise<void> {
  return invoke("update_formation_intent", { id, intent });
}

export async function addFormationMember(formationId: string, connectorName: string): Promise<void> {
  return invoke("add_formation_member", { formationId, connectorName });
}

export interface IntentInfo {
  value: string;
  label: string;
}

export async function listIntents(): Promise<IntentInfo[]> {
  return invoke<IntentInfo[]>("list_intents");
}

export interface TeamAgentSlot {
  connector_name: string;
  trigger_name: string;
  action_connector: string;
  action_name: string;
}

export interface TeamDeployRequest {
  name: string;
  intent: string;
  agents: TeamAgentSlot[];
  guard_mode: boolean;
}

export interface TeamDeployResult {
  formation_id: string;
  rule_ids: string[];
}

export async function deployTeam(team: TeamDeployRequest): Promise<TeamDeployResult> {
  return invoke<TeamDeployResult>("deploy_team", { team });
}

export async function cycleFormationIntent(id: string): Promise<string> {
  return invoke<string>("cycle_formation_intent", { id });
}

export async function cycleFormationAutonomy(id: string): Promise<string> {
  return invoke<string>("cycle_formation_autonomy", { id });
}

export async function removeFormationMember(formationId: string, connectorName: string): Promise<void> {
  return invoke("remove_formation_member", { formationId, connectorName });
}

import type { CommandDecl, MemberRef } from "@springtale/ui";

/** B11 thin-frontend API: backend-supplied 3×3 formation command grid. */
export async function formationAvailableCommands(id: string): Promise<CommandDecl[]> {
  return invoke<CommandDecl[]>("formation_commands", { id });
}

/** F5: backend-supplied eligible-removal list for the RM MBR overlay. */
export async function formationEligibleMembers(id: string): Promise<MemberRef[]> {
  return invoke<MemberRef[]>("formation_eligible_members", { id });
}
