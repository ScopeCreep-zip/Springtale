/**
 * Dashboard state types — platform-agnostic data provider interface.
 *
 * Desktop implements via Tauri invoke().
 * Web implements via HTTP fetch + SSE.
 * Both feed the same createDashboardState() factory.
 */
import type { ConnectorSchema, EventEntry, CanvasState, CanvasUpdate, AvailableConnector, ConfigSchema, ConfigSchemaProperty, AgentState } from "@springtale/types";
import type { ConnectorStatus } from "../ResourceBar";
import type { RuleItem } from "../Roster";
import type { RuleDetail, EventItem } from "../CommandPanel";
import type { ConditionDef } from "../ConditionEditor";
import type { SwarmInfo } from "../SwarmCard";

// Re-export types that originated in @springtale/types but are consumed
// by components that import from @springtale/ui
export type { AvailableConnector, ConfigSchema, ConfigSchemaProperty };

/** Wire-format rule summary from both IPC and HTTP. */
export interface RuleSummary {
  id: string;
  name: string;
  status: string;
  trigger_type: string;
  /** Connector name (from ConnectorEvent trigger). Null for cron/webhook/system triggers. */
  connector_name: string | null;
}

/** Wire-format formation info from both IPC and HTTP. */
export interface FormationInfo {
  id: string;
  name: string;
  intent: string;
  status: string;
  member_count: number;
  /** Connector names of formation members. */
  members: string[];
  /** Real momentum tier from backend: "Cold", "Warming", "Hot", "Fever". */
  momentum_tier: string;
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

  // Formations (swarms)
  listFormations(): Promise<FormationInfo[]>;
  createFormation(name: string, intent: string, connectors: string[]): Promise<string>;
  deployFormation(id: string): Promise<void>;
  pauseFormation(id: string): Promise<void>;
  resumeFormation(id: string): Promise<void>;
  dissolveFormation(id: string): Promise<void>;
  updateFormationIntent(id: string, intent: string): Promise<void>;
  addFormationMember(formationId: string, connectorName: string): Promise<void>;
  listIntents(): Promise<Array<{ value: string; label: string }>>;
  deployTeam(team: { name: string; intent: string; agents: Array<{ connector_name: string; trigger_name: string; action_connector: string; action_name: string }>; guard_mode: boolean }): Promise<{ formation_id: string; rule_ids: string[] }>;
  cycleFormationIntent(id: string): Promise<string>;
  cycleFormationAutonomy(id: string): Promise<string>;

  // Rules (extended)
  updateRule(id: string, rule: Record<string, unknown>): Promise<void>;
  runRule(id: string): Promise<{ matched: boolean }>;
  parseRuleFromIntent(intent: string): Promise<Record<string, unknown>>;
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
  getCanvasState(): Promise<CanvasState>;
  subscribeToCanvasUpdates(callback: (update: CanvasUpdate) => void): () => void;
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
  error: () => string;
  loading: () => boolean;

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

  // Derived
  selectedRule: () => RuleDetail | null;

  // Provider reference (for platform extensions needing direct access)
  provider: DataProvider;

  // Re-subscribe to SSE streams (needed after web auth config changes)
  resubscribe: () => void;
}
