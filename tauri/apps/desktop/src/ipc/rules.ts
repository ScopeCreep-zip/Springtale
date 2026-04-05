/**
 * Typed IPC wrappers for rule operations.
 */
import { invoke } from "@tauri-apps/api/core";
import type { Rule, RuleId } from "@springtale/types";

export interface RuleSummary {
  id: string;
  name: string;
  status: string;
  trigger_type: string;
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
