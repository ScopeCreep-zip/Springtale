/**
 * @springtale/ui — Shared SolidJS component library.
 *
 * Used by both the Tauri desktop app and the web dashboard.
 * Components render state and forward actions — no business logic.
 *
 * Colony network visualization: nodes=connectors, agents=porters,
 * strands=pipelines. Pixel-art aesthetic via Silkscreen font + box-shadow sprites.
 */

export { ActionPicker } from "./ActionPicker";
export type { CanvasProps } from "./Canvas";
// A2UI / Canvas block renderer — generic structured-output surface
// the bot pushes content to. Distinct from `ColonyCanvas` which
// visualises cooperation infrastructure.
export { Canvas } from "./Canvas";
export type { ConditionDef } from "./ConditionEditor";
// Form components (rendered inside colony panels)
export { ConditionEditor } from "./ConditionEditor";
// Colony panels
export { AiConfigPanel } from "./colony/AiConfigPanel";
export type { AiSchemaEditorProps } from "./colony/AiSchemaEditor";
export { AiSchemaEditor } from "./colony/AiSchemaEditor";
export type { ApprovalCardProps } from "./colony/ApprovalCard";
export { ApprovalCard } from "./colony/ApprovalCard";
export type { BotSettingsValue } from "./colony/AppSettingsPanel";
export { AppSettingsPanel } from "./colony/AppSettingsPanel";
export type { ChatDockProps } from "./colony/ChatDock";
export { ChatDock } from "./colony/ChatDock";
export type { ChatPanelProps } from "./colony/ChatPanel";
export { ChatPanel } from "./colony/ChatPanel";
export { ColonyCanvas } from "./colony/ColonyCanvas";
// Colony layout components
export { ColonyShell } from "./colony/ColonyShell";
export { ConnectorConfigPanel } from "./colony/ConnectorConfigPanel";
export type { CronFrequencyChipProps } from "./colony/CronFrequencyChip";
export { CronFrequencyChip } from "./colony/CronFrequencyChip";
export type {
  AiConfigTarget,
  ColonyControllerOptions,
  ColonyDb,
  ColonyNotification,
  ConfirmAction,
  ConnectorConfigData,
} from "./colony/controller";
export { createColonyController } from "./colony/controller";
export type { DeploySummaryModalProps } from "./colony/DeploySummaryModal";
export { DeploySummaryModal } from "./colony/DeploySummaryModal";
export type { DriftBadgeProps } from "./colony/DriftBadge";
export { DriftBadge } from "./colony/DriftBadge";
export type { EventRibbonProps } from "./colony/EventRibbon";
export { EventRibbon } from "./colony/EventRibbon";
export type { ExecutionsPanelProps } from "./colony/ExecutionsPanel";
export { ExecutionsPanel } from "./colony/ExecutionsPanel";
export type { ConnectorPositions } from "./colony/geometry";
// Colony geometry (shared position calculations for canvas + minimap)
export {
  getAgentPosition,
  getConnectorPosition,
  getFormationAgents,
  getFormationBounds,
} from "./colony/geometry";
export type { MemberPickerOverlayProps } from "./colony/MemberPickerOverlay";
export { MemberPickerOverlay } from "./colony/MemberPickerOverlay";
export type { CreateMode, ModeSelectOverlayProps } from "./colony/ModeSelectOverlay";
export { ModeSelectOverlay } from "./colony/ModeSelectOverlay";
export { MoteStack, TICK_MS } from "./colony/MoteStack";
// Colony data mappers (DashboardState → colony visual model)
export { mapAgents, mapFormations, mapNodes } from "./colony/mappers";
// Plan 3.6 — canvas overlays
export type { OverlayMode } from "./colony/overlay";
export { nextOverlay, OVERLAY_MODES, overlayColor, overlayLabel } from "./colony/overlay";
export { PendingApprovals } from "./colony/PendingApprovals";
export type { PreflightChecklistProps } from "./colony/PreflightChecklist";
export { PreflightChecklist } from "./colony/PreflightChecklist";
export type { PreviewPanelProps } from "./colony/PreviewPanel";
export { PreviewPanel } from "./colony/PreviewPanel";
export type { ProofOfLifePanelProps } from "./colony/ProofOfLifePanel";
export { ProofOfLifePanel } from "./colony/ProofOfLifePanel";
export type { BottomPanelProps } from "./colony/panel";
export { BottomPanel } from "./colony/panel";
export type { RecipeAuthorPanelProps } from "./colony/RecipeAuthorPanel";
export { RecipeAuthorPanel } from "./colony/RecipeAuthorPanel";
export type { RecipeCardProps } from "./colony/RecipeCard";
export { RecipeCard } from "./colony/RecipeCard";
export type { RecipeDeployPanelProps } from "./colony/RecipeDeployPanel";
export { RecipeDeployPanel } from "./colony/RecipeDeployPanel";
export type {
  RecipeLibraryOverlayProps,
  RecipeLibraryVariant,
} from "./colony/RecipeLibraryOverlay";
export { RecipeLibraryOverlay } from "./colony/RecipeLibraryOverlay";
export type { RecipeQuickViewProps } from "./colony/RecipeQuickView";
export { RecipeQuickView } from "./colony/RecipeQuickView";
export type { RuleBuilderOverlayProps } from "./colony/RuleBuilderOverlay";
export { RuleBuilderOverlay } from "./colony/RuleBuilderOverlay";
export type { SafetyPanelProps } from "./colony/SafetyPanel";
export { SafetyPanel } from "./colony/SafetyPanel";
export type { SelectorPickerOverlayProps } from "./colony/SelectorPickerOverlay";
export { SelectorPickerOverlay } from "./colony/SelectorPickerOverlay";
export type { TeamBuilderSeed, TeamConfig } from "./colony/TeamBuilder";
export { TeamBuilder } from "./colony/TeamBuilder";
export type { TestStepButtonProps } from "./colony/TestStepButton";
export { TestStepButton } from "./colony/TestStepButton";
export { TopBar } from "./colony/TopBar";
// Colony types + utilities
export type {
  ColonyAgent,
  ColonyCommand,
  ColonyConnection,
  ColonyFormation,
  ColonyNode,
  ColonyPipe,
  ColonySelection,
  DetailView,
} from "./colony/types";
export {
  COMMANDS,
  hash,
  MOMENTUM_COLORS,
  MOMENTUM_NAMES,
  NODE_TYPES,
  seeded,
} from "./colony/types";
export { Viewport } from "./colony/Viewport";
export type { WorkspaceTargetPickerProps } from "./colony/WorkspaceTargetPicker";
export { WorkspaceTargetPicker } from "./colony/WorkspaceTargetPicker";
export type { DisclosureSectionProps } from "./DisclosureSection";
export { DisclosureSection } from "./DisclosureSection";
export { activityOf, agentMatches, SILENT_ACTIVITY } from "./dashboard/activity";
// Dashboard state (data layer — platform-agnostic)
export { createDashboardState, DashboardProvider, useDashboard } from "./dashboard/context";
// Dashboard data model types
export type {
  ConnectorStatus,
  EventItem,
  RuleDetail,
  RuleItem,
  FormationInfo,
} from "./dashboard/model";
export type { MutationResult } from "./dashboard/query";
export { createProviderMutation, createProviderQuery } from "./dashboard/query";
export type {
  AgentHealthDetail,
  AiTarget,
  ApprovalInfo,
  AvailableConnector,
  ChatDiscoveredEvent,
  ChatStreamMessage,
  CommandDecl,
  ConfigSchema,
  ConfigSchemaProperty,
  ConnectorOutput,
  CooperationEvent,
  CooperationEventEnvelope,
  DashboardState,
  DataProvider,
  Difficulty,
  DriftReport,
  ExecutionInfo,
  ExecutionStepInfo,
  FieldKind,
  FormationDetail,
  FormationInfo,
  FormationMemberDetail,
  InputField,
  MemberRef,
  PreflightFix,
  PreflightItem,
  PreflightReport,
  PreflightStatus,
  PreviewReport,
  PreviewStep,
  Recipe,
  RecipeApplyReport,
  RecipeCategory,
  RecipeFilter,
  RecipeInputs,
  RecipePiece,
  RecipePieceSummary,
  RecipeSort,
  RecipeSource,
  RecipeSourceFilter,
  RuleSummary,
  SafetyConfig,
  SelectOption,
  TestStepReport,
  Utterance,
  UtteranceDef,
  UtteranceDefs,
  UtteranceKind,
  WorkspaceInfo,
} from "./dashboard/types";
export { createI18n, I18nProvider, useI18n } from "./i18n/context";
export type { Locale, RawDictionary } from "./i18n/types";
export { RulePreview } from "./RulePreview";
export { TriggerPicker } from "./TriggerPicker";
// HTTP client for the springtaled management API. The desktop points it
// at its sidecar daemon on loopback; the web dashboard at the host that
// served it.
export {
  configure as configureApi,
  get as apiGet,
  getBaseUrl,
  getToken,
  login as apiLogin,
  logout as apiLogout,
  put as apiPut,
} from "./web/api/client";
// Panic wipe + travel mode (§2.6) — daemon-side operations, no JSON body.
export { panicWipe, travelPrepare, travelRestore } from "./web/api/safety";
export { closeAllStreams } from "./web/api/sse";
// The one DataProvider implementation: HTTP + SSE against springtaled.
export { createWebProvider } from "./web/provider";
