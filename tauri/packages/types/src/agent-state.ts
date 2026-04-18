/**
 * Matches: springtale-runtime/src/operations/agent.rs — AgentState
 *
 * Aggregated agent state — joins rule data, recent events, and autonomy
 * into a single response. All business logic (role inference, activity
 * computation) lives in the backend.
 */
export interface AgentState {
  rule_id: string;
  name: string;
  status: string;
  trigger_type: string;
  connector_name: string | null;
  /** Agent role — inferred from trigger type by backend. */
  role: "scout" | "worker" | "guard" | "analyst";
  /** Fuel: 100 when enabled, 0 when disabled. */
  fuel: number;
  /** Activity state: "firing" | "error" | "active" | "waiting" | "idle". */
  activity: "firing" | "error" | "active" | "waiting" | "idle";
  /** Autonomy level index: 0=observe, 1=suggest, 2=approve, 3=autonomous. */
  autonomy: number;
  /** Human label for autonomy level from backend. */
  autonomy_label: string;
  /** Fuel status from backend threshold: "ok" | "warn" | "critical". */
  fuel_status: "ok" | "warn" | "critical";
  /** Pre-formatted task description from backend. */
  task_display: string;
  /** Attention load from formation's AttentionBroker (0.0-1.0). */
  attention_load: number;
  /** Liveness score (1.0 = alive, 0.0 = dead). */
  liveness: number;
  /** Health state: "healthy" | "degraded" | "incapacitated" | "dead". */
  health_state: string;
}
