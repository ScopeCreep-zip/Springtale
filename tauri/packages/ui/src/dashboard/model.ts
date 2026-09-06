/**
 * Data model types for the dashboard state layer.
 *
 * These types define the shape of data flowing from backend → provider → context → components.
 * Previously scattered across legacy component files (CommandPanel, ResourceBar, Roster, FormationCard).
 * Consolidated here so the types outlive their original rendering components.
 */

export interface ConnectorStatus {
  name: string;
  enabled: boolean;
}

export interface RuleItem {
  id: string;
  name: string;
  status: string;
  triggerType: string;
  connector?: string;
}

export interface RuleDetail {
  id: string;
  name: string;
  status: string;
  triggerType: string;
  triggerConfig: string;
  conditions: string[];
  actions: string[];
}

export interface EventItem {
  id: string;
  connectorName: string;
  triggerType: string;
  timestamp: string;
  actionTaken: string;
  /** Derived from `actionTaken` by `eventSeverity` — the `agent.rs` rule. */
  severity: "ok" | "error";
}

/**
 * The one severity rule, shared with `operations/agent.rs` (`compute_activity`):
 * an action that reports `error`, `fail`, or `block` is an error; all else ok.
 */
export function eventSeverity(actionTaken: string): EventItem["severity"] {
  const a = actionTaken.toLowerCase();
  return a.includes("error") || a.includes("fail") || a.includes("block") ? "error" : "ok";
}

export interface FormationInfo {
  id: string;
  name: string;
  intent: string;
  status: string;
  member_count: number;
  members: string[];
  operational_count?: number;
  momentum_tier?: string;
  momentum_label?: string;
  momentum_consecutive_successes?: number;
  momentum_interference_count?: number;
  momentum_successes_to_next_tier?: number | null;
  capabilities?: string[];
  guard_status?: string;
  guard_engaged?: boolean;
  rally_tokens?: number;
  rally_max?: number;
}
