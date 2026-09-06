/**
 * Dashboard state types — platform-agnostic data provider interface.
 *
 * Desktop implements via Tauri invoke().
 * Web implements via HTTP fetch + SSE.
 * Both feed the same createDashboardState() factory.
 */
import type {
  AgentState,
  ApplyReport,
  AvailableConnector,
  CanvasState,
  CanvasUpdate,
  ConfigSchema,
  ConfigSchemaProperty,
  ConnectorSchema,
  EventEntry,
  FixGuide,
  FixOutcome,
  PlatformForm,
  Report,
  SendOutcome,
  SendRequest,
  Template,
  WriteReport,
} from "@springtale/types";
import type { ConditionDef } from "../ConditionEditor";
import type { BotSettingsValue } from "../colony/AppSettingsPanel";
import type { Locale } from "../i18n/types";
import type { ConnectorStatus, EventItem, RuleDetail, RuleItem, SwarmInfo } from "./model";

// Re-export types that originated in @springtale/types but are consumed
// by components that import from @springtale/ui
export type { AvailableConnector, ConfigSchema, ConfigSchemaProperty };

/** One persisted connector-action output row. */
export interface ConnectorOutput {
  id: string;
  connector_name: string;
  rule_name: string | null;
  output_json: string;
  success: boolean;
  error_message: string | null;
  created_at: string;
}

/** A bot reply streamed to the in-app chat panel (W5). */
export interface ChatStreamMessage {
  /** Session id (channel) the reply belongs to. */
  session: string;
  /** The reply text. */
  text: string;
}

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
  /** Fuel the member started with; remaining/initial is the live fuel percentage. */
  fuel_initial: number;
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
  /** Guard badge label from the backend: "GUARD" when engaged, "--" otherwise. */
  guard_status: string;
  /** Whether the formation guard toggle is engaged. */
  guard_engaged: boolean;
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
  | {
      kind: "intervention_fired";
      formation_id: string;
      intervention:
        | { intervention: "change_intent" }
        | { intervention: "inject_fuel"; amount: number }
        | { intervention: "forced_dissolve" }
        | { intervention: "escalate_to_user" };
      summary: string;
    }
  | { kind: "pacing_phase_changed"; formation_id: string; from: string; to: string }
  | { kind: "cascade_hit"; formation_id: string; streak: number; members_affected: number }
  | { kind: "consensus_vote_opened"; formation_id: string; vote_id: string; deadline_ms: number }
  | {
      kind: "consensus_vote_resolved";
      formation_id: string;
      vote_id: string;
      outcome: "approved" | "denied" | "timeout";
    }
  | { kind: "commit_phase_changed"; formation_id: string; barrier_id: string; phase: string }
  | {
      kind: "sacrifice_yield";
      formation_id: string;
      sacrificer: string;
      beneficiary: string;
      utility: number;
    }
  | { kind: "role_transformed"; formation_id: string; agent: string; from: string; to: string }
  | { kind: "member_marked_down"; formation_id: string; agent: string; since_tick: number }
  | { kind: "supervisor_escalated"; formation_id: string; reason: string }
  | {
      kind: "recovery_action_taken";
      formation_id: string;
      helper: string;
      in_distress: string;
      action: string;
    }
  | {
      kind: "surface_deposited";
      formation_id: string;
      agent: string;
      surface_kind: string;
      ttl_ms: number;
    }
  | {
      kind: "interference_detected";
      formation_id: string;
      interference_kind:
        | "resource_conflict"
        | "action_negation"
        | "collateral_damage"
        | "task_already_claimed"
        | "duplicate_action";
      agents: string[];
    }
  | { kind: "cfp_round_started"; formation_id: string; cfp_id: string; capability: string }
  | { kind: "cfp_round_resolved"; formation_id: string; cfp_id: string; winner: string | null }
  | ({ kind: "utterance" } & Utterance)
  | { kind: "cbba_replan_requested"; formation_id: string; reason: string }
  | {
      kind: "cbba_replan_resolved";
      formation_id: string;
      outcome: { status: string; sweeps: number; assigned: number; unassigned: number };
    };

/**
 * Plan §1.15 — what was said. Mirrors `springtale_cooperation::utterance::UtteranceKind`
 * (`#[serde(tag = "utter")]`).
 */
export type UtteranceKind =
  | { utter: "firing" }
  | { utter: "working" }
  | { utter: "listening" }
  | { utter: "idle" }
  | { utter: "failed" }
  | { utter: "down" }
  | { utter: "claimed"; task: string }
  | { utter: "yield"; beneficiary: string }
  | { utter: "helping"; target: string }
  | { utter: "rally" }
  | { utter: "cascade"; streak: number };

export type UtteranceCarrier = "speech" | "burst" | "thought" | "none";
export type UtteranceShape = "triangle" | "circle" | "square";
export type UtteranceTone = "calm" | "alert" | "urgent";

/**
 * One utterance, resolved against the def table at the event site.
 * Mirrors `springtale_cooperation::utterance::Utterance`; the same fields
 * appear flat on the `kind: "utterance"` cooperation event.
 */
export interface Utterance {
  /** `null` for standalone rules (then `rule_id` is set). */
  formation_id: string | null;
  /** `null` for solo rules and formation-level kinds. */
  agent: string | null;
  rule_id: string | null;
  utterance: UtteranceKind;
  carrier: UtteranceCarrier;
  shape: UtteranceShape;
  tone: UtteranceTone;
  /** Tick it was said on — expire against the colony timeline, not wall-clock. */
  seq: number;
  ttl_ticks: number;
  glyph_frames: string[];
  mirror_rtl: boolean;
  label_key: string;
}

/**
 * One def from the table (`GET /cooperation/utterances`, desktop
 * `utterance_defs`). Mirrors `springtale_cooperation::utterance::UtteranceDef`.
 */
export interface UtteranceDef {
  carrier: UtteranceCarrier;
  shape: UtteranceShape;
  tone: UtteranceTone;
  frames: string[];
  locales: Record<string, string[]>;
  mirror_rtl: boolean;
  label_key: string;
  ttl_ticks: number;
  block_ticks: number;
}

/** Def table keyed by utterance name (`firing`, `working`, …). */
export type UtteranceDefs = Record<string, UtteranceDef>;

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

// ── W1.B Recipe library ────────────────────────────────────────

export type RecipeCategory =
  | "messaging"
  | "coding"
  | "web"
  | "ai_assistant"
  | "daily"
  | "safety_privacy"
  | "custom";

export type Difficulty = "quick" | "standard" | "power";

export type RecipeSourceFilter = "builtin" | "user" | "community";

export type RecipeSort = "recommended" | "name" | "recent";

export type RecipeSource =
  | { kind: "builtin" }
  | { kind: "user" }
  | { kind: "community"; author: string; signature: string };

export interface SelectOption {
  value: string;
  label: string;
}

export type FieldKind =
  | { kind: "text" }
  | { kind: "secret" }
  | { kind: "number" }
  | { kind: "bool" }
  | { kind: "url" }
  | { kind: "select"; options: SelectOption[] }
  | { kind: "cron" }
  | { kind: "css_selector"; sample_url?: string }
  | { kind: "json_schema"; example?: unknown }
  | { kind: "workspace_target"; connector: string; kinds?: string[] };

/**
 * Author-declared visibility per input. Mirrors the Rust
 * `FieldVisibility` enum (specta-generated when bindings.ts lands).
 * Drives progressive-disclosure tiers in `RecipeDeployPanel`.
 */
export type FieldVisibility = "required" | "optional" | "advanced" | "baked";

export interface InputField {
  id: string;
  label: string;
  kind: FieldKind;
  visibility: FieldVisibility;
  default?: unknown;
  hint?: string;
}

export interface Recipe {
  id: string;
  name: string;
  description: string;
  icon_id: string;
  category: RecipeCategory;
  tags: string[];
  connectors_used: string[];
  ai_required: boolean;
  difficulty: Difficulty;
  source: RecipeSource;
  /**
   * Single ordered list of inputs. Each carries its own
   * `FieldVisibility` so components filter the relevant tier rather
   * than depending on three separate vecs. Mirrors the Rust shape.
   */
  inputs: InputField[];
  /**
   * Blueprint passed through opaquely to `applyRecipe` — the frontend
   * doesn't introspect it; backend owns the assembly logic.
   */
  blueprint: unknown;
}

export interface RecipeFilter {
  query?: string;
  category?: RecipeCategory;
  tags?: string[];
  sources?: RecipeSourceFilter[];
  favorites_only?: boolean;
  limit?: number;
  sort?: RecipeSort;
}

/** Inputs the user supplied for a recipe — mirrors backend `RecipeInputs`. */
export interface RecipeInputs {
  values: Record<string, unknown>;
}

/** Result of applying a recipe (`POST /recipes/{id}/apply`). */
export interface RecipeApplyReport {
  recipe_id: string;
  connectors_configured: string[];
  rules_created: string[];
  ai_configured: boolean;
  summary: string;
}

// ── W1.D Preflight ────────────────────────────────────────────

export type PreflightStatus = "blocking" | "warning" | "verified" | "pending";

export type PreflightFix =
  | { kind: "focus_input"; input_id: string }
  | { kind: "open_ai_config" }
  | { kind: "open_connector_config"; connector_name: string }
  | { kind: "note"; message: string };

export interface PreflightItem {
  id: string;
  label: string;
  status: PreflightStatus;
  detail: string | null;
  fix_hint: PreflightFix | null;
}

export interface PreflightReport {
  recipe_id: string;
  items: PreflightItem[];
  deployable: boolean;
  has_warnings: boolean;
}

// ── W2.C Preview / dry-run ────────────────────────────────────

export interface PreviewStep {
  speaker: string;
  narrative: string;
  would_send_to: string | null;
}

export interface PreviewReport {
  recipe_id: string;
  steps: PreviewStep[];
  passed: boolean;
  errors: string[];
}

// ── W2.D Recipe pieces ────────────────────────────────────────

export type RecipePiece =
  | { kind: "trigger"; rule: { toml: string } }
  | { kind: "connector_config"; step: { connector_name: string; config: unknown } }
  | { kind: "ai_config"; step: { target: string; config: unknown } };

export interface RecipePieceSummary {
  id: string;
  label: string;
  piece: RecipePiece;
}

/** Mirrors `springtale_store::SafetyConfigRow` and the Tauri
 *  `SafetyConfig` IPC type. Keep all three in sync. */
export interface SafetyConfig {
  window_title: string;
  auto_lock_minutes: number;
  content_protected: boolean;
  quick_hide_shortcut: string;
  disguise_app_name: string;
  disguise_icon_id: string;
  disguise_active: boolean;
  panic_tap_count: number;
}

/**
 * Platform-agnostic data provider.
 *
 * Every method returns a Promise — callers never see platform details.
 * Subscribe methods return an unsubscribe function.
 */
/** Which level of the AI hierarchy a config applies to. Mirrors Rust `AiTarget`. */
export type AiTarget =
  | { scope: "colony" }
  | { scope: "formation"; id: string }
  | { scope: "agent"; rule_id: string };

/** One pending chat-gate approval — the shape `GET /approvals` returns (plan 6.7). */
export interface ApprovalInfo {
  id: string;
  connector_name: string;
  /** `"ShellExec"`-style manifest capability, or `{ action_type }` for a sentinel destructive action. */
  capability: string | Record<string, unknown>;
  agent_id: string | null;
  summary: string;
  requested_at: string;
  origin: { connector: string; channel_id: string } | null;
  /** Deny-by-default deadline (ISO); `null` when the gate has not stamped one. */
  expires_at: string | null;
}

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
  listConnectorOutputs(name: string, limit?: number): Promise<ConnectorOutput[]>;

  // Rules
  listRules(): Promise<RuleSummary[]>;
  createRule(rule: Record<string, unknown>): Promise<string>;
  toggleRule(id: string, enabled: boolean): Promise<void>;
  deleteRule(id: string): Promise<void>;

  // In-app chat (W5) — the desktop/web/PWA chat panel talks to the same bot
  // as the connectors. `sendChatMessage` injects a user turn; replies arrive
  // over `subscribeToChat`. Platform-agnostic: web wraps POST /chat +
  // GET /chat/stream (SSE), desktop wraps the Tauri `send_chat_message`
  // command + a chat event channel.
  sendChatMessage(text: string, session?: string): Promise<void>;
  subscribeToChat(callback: (message: ChatStreamMessage) => void): () => void;

  // Events
  listEvents(limit?: number): Promise<EventEntry[]>;
  /** Plan 6.7 — pending chat-gate approvals (`GET /approvals`). */
  listApprovals(): Promise<ApprovalInfo[]>;
  /** Plan 6.7 — approve or deny one pending approval (`POST /approvals/{id}`). */
  resolveApproval(id: string, approve: boolean): Promise<void>;
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
  /**
   * Plan §1.15 G — the utterance def table this daemon speaks with. Web:
   * `GET /cooperation/utterances`; desktop: Tauri `utterance_defs`.
   */
  getUtteranceDefs(): Promise<UtteranceDefs>;

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
  deployTeam(team: {
    name: string;
    intent: string;
    agents: Array<{
      connector_name: string;
      trigger_name: string;
      action_connector: string;
      action_name: string;
    }>;
    guard_mode: boolean;
  }): Promise<{ formation_id: string; rule_ids: string[] }>;
  cycleFormationIntent(id: string): Promise<string>;
  cycleFormationAutonomy(id: string): Promise<string>;
  /**
   * Backend-supplied 3×3 formation command grid (B11). Renders as-is per
   * thin-frontend rule — enable/disable + hotkey decided server-side
   * from formation status (`formation_available_commands` op).
   */
  formationAvailableCommands(id: string): Promise<CommandDecl[]>;
  /**
   * Generic command dispatcher — forward a clicked command id (and any picker
   * params) to the backend, which owns the command→action mapping. Keeps the
   * frontend free of per-command branching (all logic in the backend).
   */
  runFormationCommand(
    id: string,
    commandId: string,
    params?: Record<string, unknown>,
  ): Promise<void>;
  /**
   * Backend-supplied eligible-removal list for the RM MBR overlay (F5).
   */
  formationEligibleMembers(id: string): Promise<MemberRef[]>;

  // Rules (extended)
  updateRule(id: string, rule: Record<string, unknown>): Promise<void>;
  runRule(id: string): Promise<{ matched: boolean }>;
  parseRuleFromIntent(intent: string): Promise<Record<string, unknown>>;
  getRuleSchema(): Promise<Record<string, unknown>>;
  createConnectorRule(rule: {
    name: string;
    trigger_connector: string;
    trigger_event: string;
    action_connector: string;
    action_name: string;
    conditions?: unknown[];
    /** W6 chain composer — extra action steps run in order after the primary. */
    extra_actions?: { action_connector: string; action_name: string }[];
    /** W6 all-of (false, default) vs any-of (true) for the conditions. */
    match_any?: boolean;
  }): Promise<string>;
  listRulesForConnector(connectorName: string): Promise<RuleSummary[]>;
  testConnector(connectorName: string): Promise<{ matched: boolean; rule_name: string | null }>;
  reassignRuleConnector(id: string, newConnector: string): Promise<void>;

  // Safety — focused get/save on the dedicated `SafetyConfigRow` table.
  // Do NOT use `setConfig("safety", …)` for these fields: that writes
  // to a generic key/value config blob that the OS-apply commands
  // (`apply_content_protection`, `apply_disguise_to_shell`, …) do not
  // read. Routing through `getSafetyConfig` / `saveSafetyConfig`
  // hits the same table the apply commands read from, so a panel
  // Save actually takes effect at runtime.
  getSafetyConfig(): Promise<SafetyConfig>;
  saveSafetyConfig(config: SafetyConfig): Promise<void>;

  // Bot settings (plan 6.3) — persona, context window, AI tool policy.
  // Same reasoning as safety: do NOT route these through `setConfig`.
  // The dedicated endpoint validates the tool allow-list against the
  // connector registry and hot-swaps the runtime's live copy, so a save
  // takes effect on the next message instead of the next restart.
  getBotSettings(): Promise<BotSettingsValue>;
  saveBotSettings(settings: BotSettingsValue): Promise<void>;

  // Config (generic key/value, NOT for safety — see above).
  getConfig(key: string): Promise<unknown>;
  setConfig(key: string, value: unknown): Promise<void>;
  listConfig(): Promise<Array<[string, unknown]>>;
  setAiAdapter(config: Record<string, unknown>): Promise<void>;
  setConnectorConfig(name: string, config: Record<string, unknown>): Promise<void>;
  /** One config per level: colony → formation → agent (keyed by rule id). */
  configureAiAdapter(target: AiTarget, config: Record<string, unknown>): Promise<void>;
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
  getConnections(): Promise<
    Array<{ a: string; b: string; pipes: Array<{ id: string; dir: 1 | -1; status: string }> }>
  >;
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

  // W1.B — Recipe library. All filtering / sorting happens server-side;
  // frontends pass a filter, get the slice back. Favorites + recent are
  // persisted in the config store so they round-trip across surfaces.
  listRecipes(filter?: RecipeFilter): Promise<Recipe[]>;
  getRecipe(id: string): Promise<Recipe | null>;
  listRecipeCategories(): Promise<RecipeCategory[]>;
  /** Returns the new state — `true` = now a favorite. */
  toggleRecipeFavorite(recipeId: string): Promise<boolean>;
  recordRecipeRecent(recipeId: string): Promise<void>;
  // W1.C — Recipe deploy + show-as-code TOML render.
  applyRecipe(recipeId: string, inputs: RecipeInputs): Promise<RecipeApplyReport>;
  renderRecipeToml(recipeId: string, inputs: RecipeInputs): Promise<string>;
  // W1.D — Preflight checklist (live deploy-readiness validation).
  preflightRecipe(recipeId: string, inputs: RecipeInputs): Promise<PreflightReport>;
  // W2.C — Preview / dry-run with comic-strip narrative.
  previewRecipe(recipeId: string, inputs: RecipeInputs): Promise<PreviewReport>;
  // W2.D — Borrow named pieces (trigger / connector config / AI) from a recipe.
  listRecipePieces(recipeId: string): Promise<RecipePieceSummary[]>;
  // W2.B — Recipe authoring (save / fork / delete / export / import).
  saveUserRecipe(recipe: Recipe): Promise<Recipe>;
  forkRecipe(recipeId: string, newName: string): Promise<Recipe>;
  deleteUserRecipe(recipeId: string): Promise<boolean>;
  exportRecipeToml(recipeId: string): Promise<string>;
  importRecipeToml(toml: string): Promise<Recipe>;

  // Templates
  listTemplates(): Promise<Template[]>;
  writeTemplate(name: string): Promise<WriteReport>;

  // Error fixes
  listFixes(): Promise<FixGuide[]>;
  getFix(id: string): Promise<FixGuide>;
  applyFix(id: string): Promise<FixOutcome>;

  // Cross-channel send
  sendMessage(req: SendRequest): Promise<SendOutcome>;

  // Phase B — executions log (privacy-default observability).
  // Sizes-only. Content retention is opt-in (Phase C) and exposed
  // through `*_blob_ref` fields, never inlined.
  listExecutions(filter: ExecutionFilterInput): Promise<ExecutionInfo[]>;
  getExecutionSteps(executionId: string): Promise<ExecutionStepInfo[]>;

  // Phase B — selector picker (authoring-time tool).
  // Opens a Tauri webview at `url`, returns the picked CSS
  // selector (or null when the user cancels). Web provider may
  // return null without ever opening a window — selector picking
  // requires a desktop webview to work safely.
  openSelectorPicker(url: string, hostAllowlist: string[]): Promise<string | null>;

  // Phase C — per-step dry-run + drift detection.
  // Test This Step: fires the recipe's rule[rule_index] in DryRun
  // mode through actions[0..=step_index], returns the targeted
  // step's recorded output. Side-effecting steps stub; read steps
  // (HTTP get, browser navigate, AiComplete, Extract, Dedupe
  // check-only) run for real so the UI sees realistic upstream
  // data.
  testRecipeStep(
    recipeId: string,
    inputs: RecipeInputs,
    ruleIndex: number,
    stepIndex: number,
  ): Promise<TestStepReport>;
  // Drift: aggregates the recipe's most-recent runs from the
  // executions log into latency / success-rate / refusal-rate
  // trends. Frontend renders as a DriftBadge per recipe row.
  getRecipeDrift(recipeId: string, filter: DriftFilterInput): Promise<DriftReport>;

  // D1 — External-workspace directory (the formation's
  // gossip-replicated yellow pages of messaging destinations).
  listWorkspaces(formationId: string, connectorFilter?: string): Promise<WorkspaceInfo[]>;
  scanWorkspaces(formationId: string, connectorName: string): Promise<WorkspaceInfo[]>;
  deleteWorkspace(formationId: string, workspaceKey: string): Promise<void>;
  upsertWorkspaceManual(
    formationId: string,
    workspaceKey: string,
    displayName: string,
    connectorName: string,
    kind: string,
  ): Promise<void>;
  /** Pre-deploy onboarding URL resolver.
   *
   *  Connector-agnostic. Hands the deploy form's connector config to
   *  the connector factory which spins up a one-shot instance and
   *  dispatches the connector's `onboard_url` action. Telegram
   *  returns a `t.me/<bot>?start=…` deep link; other connectors that
   *  implement `onboard_url` plug into the same path without any
   *  frontend changes. Rejects (caller catches) when the connector
   *  has no `onboard_url` action. */
  previewOnboardUrl(
    connectorName: string,
    config: Record<string, unknown>,
    payload?: string,
  ): Promise<string>;

  /** Track D — kick off the 60s auto-onboard stream.
   *
   *  Companion to `previewOnboardUrl`. While the user has the
   *  onboarding link copied and is in Telegram tapping START, the
   *  backend polls the connector's `discover_destinations` action
   *  every 2 seconds. Each match fires a `chat-discovered` event
   *  tagged with `sessionId`. Subscribe via `subscribeToChatDiscovered`
   *  BEFORE invoking this so the listener doesn't miss the first
   *  emission. */
  startOnboardStream(
    sessionId: string,
    connectorName: string,
    config: Record<string, unknown>,
    payload?: string,
  ): Promise<void>;

  /** Track D — tear down an active onboard stream. Idempotent. */
  cancelOnboardStream(sessionId: string): Promise<void>;

  /** Track D — subscribe to `chat-discovered` events. Returns the
   *  unlisten function (no-op on web). */
  subscribeToChatDiscovered(callback: (event: ChatDiscoveredEvent) => void): Promise<() => void>;
}

/** Payload of the `chat-discovered` Tauri event (Track D). */
export interface ChatDiscoveredEvent {
  session_id: string;
  workspace_key: string;
  display_name: string;
  kind: string;
  metadata_json: string | null;
  /** `true` when the discovery passed the `/start <payload>` filter
   *  (i.e. it is the user's own onboarding tap). */
  matched: boolean;
}

/**
 * One row in the formation's external-workspace directory
 * (`mental_model_workspaces`). Sizes-only metadata —
 * `display_name` is the only human-readable text persisted.
 */
export interface WorkspaceInfo {
  /** URI form — `"telegram://chat/12345"`, etc. */
  workspace_key: string;
  connector_name: string;
  display_name: string;
  /** `"user" | "group" | "channel" | "supergroup" | "dm" | "account" | "thread"`. */
  kind: string;
  /** Connector-specific extras serialized as JSON string. */
  metadata_json: string | null;
  first_seen_at_unix_ms: number;
  last_seen_at_unix_ms: number;
  /** Serialized `WorkspaceProvenance` enum. Frontend parses for
   *  the tooltip "discovered by … via …" copy. */
  provenance_json: string;
}

/**
 * Test This Step result. Mirrors
 * `springtale-runtime::operations::test_step::TestStepReport`.
 */
export interface TestStepReport {
  recipe_id: string;
  rule_index: number;
  step_index: number;
  ran: boolean;
  step: TestStepOutput | null;
  upstream: TestStepOutput[];
  error: string | null;
}

export interface TestStepOutput {
  index: number;
  kind: string;
  name: string | null;
  /** JSON output rendered as a string. Parse client-side when needed. */
  output_json: string;
  duration_ms: number;
  error: string | null;
}

/**
 * Drift filter. All fields optional; `recent_n` / `baseline_n`
 * default to 10 / 30 server-side.
 */
export interface DriftFilterInput {
  bot_id?: string;
  formation_id?: string;
  rule_id?: string;
  recent_n?: number;
  baseline_n?: number;
}

/**
 * Drift verdict per signal — `not_enough_data` hides the badge,
 * `steady` shows a neutral chip, `improving` / `degrading` colour
 * the chip accordingly.
 */
export type DriftClass = "not_enough_data" | "steady" | "improving" | "degrading";

export interface DriftReport {
  recent_runs: number;
  baseline_runs: number;
  latency: LatencyDrift;
  success_rate: RateDrift;
  refusal_rate: RateDrift;
  overall: DriftClass;
}

export interface LatencyDrift {
  recent_median_ms: number | null;
  recent_p95_ms: number | null;
  baseline_median_ms: number | null;
  baseline_p95_ms: number | null;
  median_delta_ms: number | null;
  class: DriftClass;
}

export interface RateDrift {
  recent: number | null;
  baseline: number | null;
  delta: number | null;
  class: DriftClass;
}

/**
 * Executions log filter. All fields optional; `before` is a
 * unix-ms cursor on `started_at` for pagination (return rows older
 * than this). `limit` caps at 500 server-side.
 */
export interface ExecutionFilterInput {
  bot_id?: string;
  formation_id?: string;
  rule_id?: string;
  status?: ExecutionStatusTag;
  before?: number;
  limit?: number;
}

export type ExecutionStatusTag =
  | "running"
  | "succeeded"
  | "empty"
  | "failed"
  | "aborted"
  | "timed_out";

export type ExecutionModeTag =
  | "cron"
  | "webhook"
  | "connector_event"
  | "file_watch"
  | "manual"
  | "cooperation"
  | "retry"
  | "dry_run";

export type StepStatusTag = "succeeded" | "failed" | "suppressed" | "skipped";

export type MomentumTag = "cold" | "warming" | "hot" | "fever";

/** One row in the executions list — sizes-only summary. */
export interface ExecutionInfo {
  id: string;
  bot_id: string | null;
  formation_id: string | null;
  rule_id: string | null;
  recipe_id: string | null;
  started_at: number;
  finished_at: number | null;
  duration_ms: number | null;
  mode: ExecutionModeTag;
  status: ExecutionStatusTag;
  momentum: MomentumTag | null;
  trigger_summary: string | null;
  error_kind: string | null;
}

/** One step inside an execution. Sizes-only; content opt-in via blob refs. */
export interface ExecutionStepInfo {
  execution_id: string;
  step_index: number;
  step_kind: string;
  connector: string | null;
  action: string | null;
  started_at: number;
  finished_at: number | null;
  status: StepStatusTag;
  input_bytes: number;
  output_bytes: number;
  output_kind: string | null;
  error_kind: string | null;
  input_blob_ref: string | null;
  output_blob_ref: string | null;
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
  /** Plan 3.4 — utterance events on the ring, newest first. */
  utterances: () => Utterance[];
  /** Newest tick any utterance was said on; motes expire against it. */
  colonyNow: () => number;
  formationDetails: () => FormationDetail[];
  agentToConnector: () => Record<string, string>;
  roleOf: (agentId: string) => string | undefined;
  framesFor: (u: Utterance, locale: Locale) => string[];
  /** Plan 6.7 — pending approvals, reloaded on `approval_required` events and after each resolve. */
  pendingApprovals: () => ApprovalInfo[];
  refreshApprovals: () => Promise<void>;
  resolveApproval: (id: string, approve: boolean) => Promise<void>;
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
