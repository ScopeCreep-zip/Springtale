/**
 * Formation types — mirrors cooperation module from springtale-bot.
 *
 * These types represent the cooperative agent architecture:
 * formations of agents coordinating through cadence, momentum,
 * and awareness. The RTS-style dashboard visualizes these as
 * "units" on a command canvas.
 */

export type MomentumTier = "Cold" | "Warming" | "Hot" | "Fever";

export type AgentHealth =
  | { type: "Operational" }
  | { type: "Degraded"; recovery_count: number }
  | { type: "Incapacitated" }
  | { type: "Dead"; recoverable: boolean };

export type DynamicRole =
  | { type: "Unassigned" }
  | { type: "Primary"; task: string }
  | { type: "Support"; supporting: string }
  | { type: "Information" }
  | { type: "Custom"; name: string };

export interface FormationMember {
  agent_id: string;
  capabilities: string[];
  current_role: DynamicRole;
  attention_load: number;
  health: AgentHealth;
}

export interface FormationSummary {
  id: string;
  member_count: number;
  operational_count: number;
  momentum_tier: MomentumTier;
  intent: string;
  is_viable: boolean;
  /** Rally tokens remaining (Monster Hunter carts, §15). */
  rally_tokens: number;
  /** Maximum rally tokens. */
  rally_max: number;
}

export interface FormationDetail {
  id: string;
  members: FormationMember[];
  momentum_tier: MomentumTier;
  intent: string;
  is_viable: boolean;
  /** Rally tokens remaining (Monster Hunter carts, §15). */
  rally_tokens: number;
  /** Maximum rally tokens. */
  rally_max: number;
}

export type IntentPattern =
  | { type: "Reconnoiter"; target: string }
  | { type: "Execute"; plan_id?: string }
  | { type: "Stabilize"; reason: string }
  | { type: "Surge"; objective: string }
  | { type: "Dissolve"; reason: string };
