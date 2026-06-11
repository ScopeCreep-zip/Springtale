/**
 * Typed IPC wrappers for rule operations.
 */

import { invoke } from "@tauri-apps/api/core";

export interface RuleSummary {
  id: string;
  name: string;
  status: string;
  trigger_type: string;
  connector_name: string | null;
}

export async function listRules(): Promise<RuleSummary[]> {
  return invoke<RuleSummary[]>("list_rules");
}

export async function toggleRule(id: string, enabled: boolean): Promise<void> {
  return invoke("toggle_rule", { id, enabled });
}

export async function deleteRule(id: string): Promise<void> {
  return invoke("delete_rule", { id });
}

export async function createRule(rule: Record<string, unknown>): Promise<string> {
  return invoke<string>("create_rule", { rule });
}

export async function updateRule(id: string, rule: Record<string, unknown>): Promise<void> {
  return invoke("update_rule", { id, rule });
}

export async function runRule(id: string): Promise<{ matched: boolean }> {
  return invoke("run_rule", { id });
}

export async function parseRuleFromIntent(intent: string): Promise<Record<string, unknown>> {
  return invoke("parse_rule", { intent });
}

export async function createConnectorRule(rule: {
  name: string;
  trigger_connector: string;
  trigger_event: string;
  action_connector: string;
  action_name: string;
  conditions?: unknown[];
  extra_actions?: { action_connector: string; action_name: string }[];
  match_any?: boolean;
}): Promise<string> {
  return invoke<string>("create_connector_rule", { rule });
}

export async function getRuleSchema(): Promise<Record<string, unknown>> {
  return invoke("get_rule_schema");
}

export async function listRulesForConnector(connectorName: string): Promise<RuleSummary[]> {
  return invoke<RuleSummary[]>("list_rules_for_connector", { connectorName });
}

export async function testConnector(
  connectorName: string,
): Promise<{ matched: boolean; rule_name: string | null }> {
  return invoke("test_connector", { connectorName });
}

export async function reassignRuleConnector(id: string, newConnector: string): Promise<void> {
  return invoke("reassign_rule_connector", { id, newConnector });
}
