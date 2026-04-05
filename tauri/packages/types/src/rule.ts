/**
 * Matches: springtale-core/src/rule/types.rs — Rule, RuleStatus, RuleId
 */
export type RuleId = string; // UUID

export type RuleStatus = "enabled" | "disabled" | "draft";

export interface Rule {
  id: RuleId;
  name: string;
  description: string;
  status: RuleStatus;
  version: number;
  trigger: unknown; // Complex enum — use JSON for now
  conditions: unknown[];
  actions: unknown[];
}
