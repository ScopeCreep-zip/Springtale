/**
 * Dashboard state types — platform-agnostic data provider interface.
 *
 * Desktop implements via Tauri invoke().
 * Web implements via HTTP fetch + SSE.
 * Both feed the same createDashboardState() factory.
 */
import type { ConnectorSchema, EventEntry, CanvasState, CanvasUpdate } from "@springtale/types";
import type { ConnectorStatus } from "../ResourceBar";
import type { RuleItem } from "../Roster";
import type { RuleDetail, EventItem } from "../CommandPanel";
import type { ConditionDef } from "../ConditionEditor";
import type { SwarmInfo } from "../SwarmCard";

/** Wire-format rule summary from both IPC and HTTP. */
export interface RuleSummary {
  id: string;
  name: string;
  status: string;
  trigger_type: string;
}

/** Wire-format formation info from both IPC and HTTP. */
export interface FormationInfo {
  id: string;
  name: string;
  intent: string;
  status: string;
  member_count: number;
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
  getConnectorSchemas(): Promise<ConnectorSchema[]>;

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
  showHatchWizard: () => boolean;
  setShowHatchWizard: (v: boolean) => void;

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
