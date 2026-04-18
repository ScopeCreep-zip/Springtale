/**
 * Data model types for the dashboard state layer.
 *
 * These types define the shape of data flowing from backend → provider → context → components.
 * Previously scattered across legacy component files (CommandPanel, ResourceBar, Roster, SwarmCard).
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
  /** Severity from backend: "ok" | "error". */
  severity: "ok" | "error";
}

export interface SwarmInfo {
  id: string;
  name: string;
  intent: string;
  status: string;
  member_count: number;
  members: string[];
  momentum_tier?: string;
  momentum_label?: string;
  capabilities?: string[];
  guard_status?: string;
  rally_tokens?: number;
  rally_max?: number;
}
