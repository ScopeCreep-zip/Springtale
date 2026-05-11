/**
 * Dashboard state types — platform-agnostic data provider interface.
 *
 * Desktop implements via Tauri invoke().
 * Web implements via HTTP fetch + SSE.
 * Both feed the same createDashboardState() factory.
 */
import type {
  ConnectorSchema, EventEntry, CanvasState, CanvasUpdate, AvailableConnector,
  ConfigSchema, ConfigSchemaProperty, AgentState,
  Report, PlatformForm, ApplyReport, Template, WriteReport,
  FixGuide, FixOutcome, SendRequest, SendOutcome,
} from "@springtale/types";
import type { ConnectorStatus, RuleItem, RuleDetail, EventItem, SwarmInfo } from "./model";
import type { ConditionDef } from "../ConditionEditor";

// Re-export types that originated in @springtale/types but are consumed
// by components that import from @springtale/ui
export type { AvailableConnector, ConfigSchema, ConfigSchemaProperty };

/** Wire-format rule summary from both IPC and HTTP. */
export interface RuleSummary {
  id: string;
  name: string;
  status: string;
  trigger_type: string;
  /** Connector name (from trigger). Null for cron/webhook/system triggers. */
  connector_name: string | null;
}

/** Structured agent health — tagged union matching backend
 *  `AgentHealthDetail`. Replaces the old stringified Debug output. */
export type AgentHealthDetail =
  | { type: "Operational" }
  | { type: "Degraded"; recovery_count: number }
  | { type: "Incapacitated" }
  | { type: "Dead"; recoverable: boolean };

/** Enriched per-member detail from live formation data. */
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

/** Enriched formation detail — FormationInfo plus live member details. */
export interface FormationDetail extends FormationInfo {
  member_details: FormationMemberDetail[];
}

/** Wire-format formation info from both IPC and HTTP. */
export interface FormationInfo {
  id: string;
  name: string;
  intent: string;
  status: string;
  member_count: number;
  /** Members whose health is Operational or Degraded (able to carry work). */
  operational_count: number;
  /** Connector names of formation members. */
  members: string[];
  /** Real momentum tier from backend: "Cold", "Warming", "Hot", "Fever". */
  momentum_tier: string;
  momentum_label?: string;
  /** Consecutive successful ticks in the current run. */
  momentum_consecutive_successes?: number;
  /** Interference count in the current tier. */
  momentum_interference_count?: number;
  /** Successes remaining to promote; null at Fever (top tier). */
  momentum_successes_to_next_tier?: number | null;
  capabilities?: string[];
  guard_status?: string;
  /** Rally tokens remaining (Monster Hunter carts, §15). */
  rally_tokens?: number;
  rally_max?: number;
}

/**
 * Backend-supplied formation command descriptor (B11). The frontend
 * renders the list as-is; eligibility / hotkeys / labels are decided
 * server-side per the thin-frontend rule.
 */
export interface CommandDecl {
  /** Stable command id, e.g. `"formation:deploy"`. Frontend dispatches by this. */
  id: string;
  /** Human label shown on the button, e.g. `"DEPLOY"`. */
  label: string;
  /** Pixel icon character. */
  icon: string;
  /** Canonical hotkey decided server-side so it's the same on every surface. */
  hotkey: string;
  /** Whether the command is currently usable. */
  enabled: boolean;
  /** Reason shown when `enabled = false` (tooltip / aria-label). */
  disabled_reason: string | null;
}

/**
 * Phase H cooperation events — the user-observable side of internal
 * cooperation state changes (interventions firing, sacrifices yielded,
 * votes opened, etc.). Mirrors `springtale_cooperation::CooperationEvent`
 * one-for-one; serde uses `kind` as the discriminator (snake_case).
 */
export type CooperationEvent =
  | { kind: "intervention_fired"; formation_id: string; intervention: { intervention: "change_intent" } | { intervention: "inject_fuel"; amount: number } | { intervention: "forced_dissolve" } | { intervention: "escalate_to_user" }; summary: string }
  | { kind: "pacing_phase_changed"; formation_id: string; from: string; to: string }
  | { kind: "cascade_hit"; formation_id: string; streak: number; members_affected: number }
  | { kind: "consensus_vote_opened"; formation_id: string; vote_id: string; deadline_ms: number }
  | { kind: "consensus_vote_resolved"; formation_id: string; vote_id: string; outcome: "approved" | "denied" | "timeout" }
  | { kind: "commit_phase_changed"; formation_id: string; barrier_id: string; phase: string }
  | { kind: "sacrifice_yield"; formation_id: string; sacrificer: string; beneficiary: string; utility: number }
  | { kind: "role_transformed"; formation_id: string; agent: string; from: string; to: string }
  | { kind: "member_marked_down"; formation_id: string; agent: string; since_tick: number }
  | { kind: "supervisor_escalated"; formation_id: string; reason: string }
  | { kind: "recovery_action_taken"; formation_id: string; helper: string; in_distress: string; action: string }
  | { kind: "surface_deposited"; formation_id: string; agent: string; surface_kind: string; ttl_ms: number }
  | { kind: "interference_detected"; formation_id: string; interference_kind: "resource_conflict" | "action_negation" | "collateral_damage" | "task_already_claimed" | "duplicate_action"; agents: string[] }
  | { kind: "cfp_round_started"; formation_id: string; cfp_id: string; capability: string }
  | { kind: "cfp_round_resolved"; formation_id: string; cfp_id: string; winner: string | null }
  | { kind: "cbba_replan_requested"; formation_id: string; reason: string }
  | { kind: "cbba_replan_resolved"; formation_id: string; outcome: { status: string; sweeps: number; assigned: number; unassigned: number } };

/**
 * Wire envelope wrapping every cooperation event — adds monotonic seq +
 * UTC timestamp. SSE/Channel subscribers see this shape.
 */
export interface CooperationEventEnvelope {
  seq: number;
  at: string;
  event: CooperationEvent;
}

/**
 * Backend-supplied eligible-removal target for the RM MBR overlay (F5).
 */
export interface MemberRef {
  agent_id: string;
  connector_name: string;
  role: string;
  can_remove: boolean;
  block_reason: string | null;
}

/**
 * Platform-agnostic data provider.
 *
 * Every method returns a Promise — callers never see platform details.
 * Subscribe methods return an unsubscribe function.
 */
export interface DataProvider {
  // Connectors
  listConnectors(): Promise<Array<{ name: string; enabled: boolean }>>;
  listAvailableConnectors(): Promise<AvailableConnector[]>;
  setupConnector(name: string, config: Record<string, unknown>): Promise<string>;
  getConnectorSchemas(): Promise<ConnectorSchema[]>;
  enableConnector(name: string): Promise<void>;
  disableConnector(name: string): Promise<void>;
  /**
   * G4 — hot-reload a connector. Backend rebuilds the host atomically;
   * in-flight calls finish on the old instance and subsequent calls
   * land on the new one. Frontend just dispatches.
   */
  reloadConnector(name: string): Promise<void>;
  removeConnector(name: string): Promise<void>;
  removeConnectorCascade(name: string): Promise<string[]>;
  getConnectorConfig(name: string): Promise<unknown>;
  listConnectorOutputs(name: string, limit?: number): Promise<Array<{
    id: string;
    connector_name: string;
    rule_name: string | null;
    output_json: string;
    success: boolean;
    error_message: string | null;
    created_at: string;
  }>>;

  // Rules
  listRules(): Promise<RuleSummary[]>;
  createRule(rule: Record<string, unknown>): Promise<string>;
  toggleRule(id: string, enabled: boolean): Promise<void>;
  deleteRule(id: string): Promise<void>;

  // Events
  listEvents(limit?: number): Promise<EventEntry[]>;
  subscribeToEvents(callback: (event: EventEntry) => void): () => void;
  /**
   * Phase H cooperation events stream — internal-state lifecycle envelopes
   * (intervention fired, sacrifice yielded, vote opened, role transformed,
   * member marked down, supervisor escalation, pacing phase change,
   * cascade hit, recovery action, surface deposit, interference event,
   * CFP/replan/commit outcome). Web subscribes via SSE
   * `/cooperation/events`; desktop subscribes via Tauri
   * `subscribe_cooperation` Channel<CooperationEventEnvelope>.
   */
  subscribeToCooperationEvents(callback: (envelope: CooperationEventEnvelope) => void): () => void;

  // Formations (swarms)
  getFormation(id: string): Promise<FormationDetail>;
  listFormations(): Promise<FormationInfo[]>;
  createFormation(name: string, intent: string, connectors: string[]): Promise<string>;
  deployFormation(id: string): Promise<void>;
  pauseFormation(id: string): Promise<void>;
  resumeFormation(id: string): Promise<void>;
  dissolveFormation(id: string): Promise<void>;
  rallyFormation(id: string): Promise<void>;
  updateFormationIntent(id: string, intent: string): Promise<void>;
  addFormationMember(formationId: string, connectorName: string): Promise<void>;
  removeFormationMember(formationId: string, connectorName: string): Promise<void>;
  listIntents(): Promise<Array<{ value: string; label: string }>>;
  deployTeam(team: { name: string; intent: string; agents: Array<{ connector_name: string; trigger_name: string; action_connector: string; action_name: string }>; guard_mode: boolean }): Promise<{ formation_id: string; rule_ids: string[] }>;
  cycleFormationIntent(id: string): Promise<string>;
  cycleFormationAutonomy(id: string): Promise<string>;
  /**
   * Backend-supplied 3×3 formation command grid (B11). Renders as-is per
   * thin-frontend rule — enable/disable + hotkey decided server-side
   * from formation status (`formation_available_commands` op).
   */
  formationAvailableCommands(id: string): Promise<CommandDecl[]>;
  /**
   * Backend-supplied eligible-removal list for the RM MBR overlay (F5).
   */
  formationEligibleMembers(id: string): Promise<MemberRef[]>;

  // Rules (extended)
  updateRule(id: string, rule: Record<string, unknown>): Promise<void>;
  runRule(id: string): Promise<{ matched: boolean }>;
  parseRuleFromIntent(intent: string): Promise<Record<string, unknown>>;
  getRuleSchema(): Promise<Record<string, unknown>>;
  createConnectorRule(rule: { name: string; trigger_connector: string; trigger_event: string; action_connector: string; action_name: string; conditions?: unknown[] }): Promise<string>;
  listRulesForConnector(connectorName: string): Promise<RuleSummary[]>;
  testConnector(connectorName: string): Promise<{ matched: boolean; rule_name: string | null }>;
  reassignRuleConnector(id: string, newConnector: string): Promise<void>;

  // Config
  getConfig(key: string): Promise<unknown>;
  setConfig(key: string, value: unknown): Promise<void>;
  listConfig(): Promise<Array<[string, unknown]>>;
  setAiAdapter(config: Record<string, unknown>): Promise<void>;
  setConnectorConfig(name: string, config: Record<string, unknown>): Promise<void>;
  configureAiAdapter(target: string, config: Record<string, unknown>): Promise<void>;
  upsertConnectorConfig(name: string, config: Record<string, unknown>): Promise<boolean>;
  toggleFormationGuard(formationId: string): Promise<boolean>;

  // Agent state + autonomy
  listAgentStates(): Promise<AgentState[]>;
  getAutonomy(name: string): Promise<string>;
  setAutonomy(name: string, level: string): Promise<void>;
  stepAutonomy(name: string, direction: "up" | "down"): Promise<string>;

  // Trusted authors
  listAuthors(): Promise<Array<{ name: string; pubkey: string }>>;
  addAuthor(name: string, pubkey: string): Promise<void>;
  removeAuthor(name: string): Promise<void>;

  // Data management
  exportData(): Promise<unknown>;

  // Memory management
  auditMemory(): Promise<unknown>;
  compactMemory(maxEntries: number): Promise<void>;

  // Canvas
  getConnections(): Promise<Array<{ a: string; b: string; pipes: Array<{ id: string; dir: 1 | -1; status: string }> }>>;
  getCanvasState(): Promise<CanvasState>;
  subscribeToCanvasUpdates(callback: (update: CanvasUpdate) => void): () => void;

  /**
   * G5d — IPV duress surface: toggle whether the app currently
   * renders its disguise UI. Persisted server-side; survives
   * restart. The frontend just dispatches; whether to swap the OS
   * tray icon / launcher name lives in the platform shell.
   */
  setDisguiseActive(active: boolean): Promise<boolean>;

  /**
   * G5d — atomically update which disguise profile (app name +
   * icon id) the platform shell should display when
   * `disguiseActive` is true.
   */
  setDisguiseProfile(appName: string, iconId: string): Promise<void>;

  /**
   * G5d — set the panic-tap threshold. `count = 0` disables the
   * gesture; values out of `[0, 10]` are rejected server-side
   * (HTTP 400) to prevent rendering panic-wipe unreachable.
   */
  setPanicTapCount(count: number): Promise<number>;

  // Diagnostics
  runDiagnostics(): Promise<Report>;

  // Onboarding
  listOnboardingPlatforms(): Promise<PlatformForm[]>;
  applyOnboarding(platform: string, answers: Record<string, string>): Promise<ApplyReport>;

  // Templates
  listTemplates(): Promise<Template[]>;
  writeTemplate(name: string): Promise<WriteReport>;

  // Error fixes
  listFixes(): Promise<FixGuide[]>;
  getFix(id: string): Promise<FixGuide>;
  applyFix(id: string): Promise<FixOutcome>;

  // Cross-channel send
  sendMessage(req: SendRequest): Promise<SendOutcome>;
}

/**
 * Return type of createDashboardState().
 *
 * Contains all reactive signals and action handlers the Dashboard needs.
 * Platform-specific features (vault, settings) are NOT included — those
 * stay in each app's local state.
 */
export interface DashboardState {
  // Core data signals (read-only)
  connectors: () => ConnectorStatus[];
  schemas: () => ConnectorSchema[];
  rules: () => RuleItem[];
  events: () => EventItem[];
  swarms: () => SwarmInfo[];
  agentStates: () => AgentState[];
  canvasState: () => CanvasState | null;
  /**
   * Phase H — last 200 cooperation event envelopes (most-recent first).
   * Drives the EventRibbon toast (high-severity filter) and the
   * BottomPanel formation event log (per-formation filter).
   */
  cooperationEvents: () => CooperationEventEnvelope[];
  error: () => string;
  loading: () => boolean;
  /**
   * F1 + B11: backend-supplied formation command grid for the current
   * `selectedSwarmId`. Resource that re-fetches when selection changes.
   * Returns `undefined` when no formation is selected.
   */
  formationCommands: () => CommandDecl[] | undefined;

  // Selection state
  selectedRuleId: () => string | null;
  setSelectedRuleId: (id: string | null) => void;
  selectedSwarmId: () => string | null;
  setSelectedSwarmId: (id: string | null) => void;

  // UI panel state
  showNewRule: () => boolean;
  setShowNewRule: (v: boolean) => void;
  // Rule builder form state
  newRuleName: () => string;
  setNewRuleName: (v: string) => void;
  triggerConnector: () => string;
  setTriggerConnector: (v: string) => void;
  triggerName: () => string;
  setTriggerName: (v: string) => void;
  actionConnector: () => string;
  setActionConnector: (v: string) => void;
  actionName: () => string;
  setActionName: (v: string) => void;
  conditions: () => ConditionDef[];
  setConditions: (v: ConditionDef[]) => void;

  // Error management
  setError: (msg: string) => void;
  clearError: () => void;

  // Actions
  refresh: () => Promise<void>;
  handleToggle: (id: string, currentlyEnabled: boolean) => Promise<void>;
  handleDelete: (id: string) => Promise<void>;
  handleSaveNewRule: () => Promise<void>;
  handleHatch: (
    name: string,
    intent: string,
    connectors: string[],
    trigC: string,
    trigE: string,
    actC: string,
    actN: string,
  ) => Promise<void>;
  handleDeployFormation: (id: string) => Promise<void>;
  handlePauseFormation: (id: string) => Promise<void>;
  handleResumeFormation: (id: string) => Promise<void>;
  handleDissolveFormation: (id: string) => Promise<void>;
  handleRallyFormation: (id: string) => Promise<void>;

  // Derived
  selectedRule: () => RuleDetail | null;

  // Provider reference (for platform extensions needing direct access)
  provider: DataProvider;

  // Re-subscribe to SSE streams (needed after web auth config changes)
  resubscribe: () => void;
}
