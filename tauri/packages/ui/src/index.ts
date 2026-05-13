/**
 * @springtale/ui — Shared SolidJS component library.
 *
 * Used by both the Tauri desktop app and the web dashboard.
 * Components render state and forward actions — no business logic.
 *
 * Colony network visualization: nodes=connectors, agents=porters,
 * strands=pipelines. Pixel-art aesthetic via Silkscreen font + box-shadow sprites.
 */

export { createI18n, I18nProvider, useI18n } from "./i18n/context";
export type { Locale, RawDictionary } from "./i18n/types";

// Colony layout components
export { ColonyShell } from "./colony/ColonyShell";
export { ModeSelectOverlay } from "./colony/ModeSelectOverlay";
export type { ModeSelectOverlayProps, CreateMode } from "./colony/ModeSelectOverlay";
export { RecipeLibraryOverlay } from "./colony/RecipeLibraryOverlay";
export type { RecipeLibraryOverlayProps, RecipeLibraryVariant } from "./colony/RecipeLibraryOverlay";
export { RecipeCard } from "./colony/RecipeCard";
export type { RecipeCardProps } from "./colony/RecipeCard";
export { RecipeQuickView } from "./colony/RecipeQuickView";
export type { RecipeQuickViewProps } from "./colony/RecipeQuickView";
export { RecipeDeployPanel } from "./colony/RecipeDeployPanel";
export type { RecipeDeployPanelProps } from "./colony/RecipeDeployPanel";
export { DisclosureSection } from "./DisclosureSection";
export type { DisclosureSectionProps } from "./DisclosureSection";
export { PreflightChecklist } from "./colony/PreflightChecklist";
export type { PreflightChecklistProps } from "./colony/PreflightChecklist";
export { ProofOfLifePanel } from "./colony/ProofOfLifePanel";
export type { ProofOfLifePanelProps } from "./colony/ProofOfLifePanel";
export { ApprovalCard } from "./colony/ApprovalCard";
export type { ApprovalCardProps } from "./colony/ApprovalCard";
export { PreviewPanel } from "./colony/PreviewPanel";
export type { PreviewPanelProps } from "./colony/PreviewPanel";
export { RecipeAuthorPanel } from "./colony/RecipeAuthorPanel";
export type { RecipeAuthorPanelProps } from "./colony/RecipeAuthorPanel";
export { MemberPickerOverlay } from "./colony/MemberPickerOverlay";
export type { MemberPickerOverlayProps } from "./colony/MemberPickerOverlay";
export { SelectorPickerOverlay } from "./colony/SelectorPickerOverlay";
export type { SelectorPickerOverlayProps } from "./colony/SelectorPickerOverlay";
export { ExecutionsPanel } from "./colony/ExecutionsPanel";
export type { ExecutionsPanelProps } from "./colony/ExecutionsPanel";
export { AiSchemaEditor } from "./colony/AiSchemaEditor";
export type { AiSchemaEditorProps } from "./colony/AiSchemaEditor";
export { TestStepButton } from "./colony/TestStepButton";
export type { TestStepButtonProps } from "./colony/TestStepButton";
export { DeploySummaryModal } from "./colony/DeploySummaryModal";
export type { DeploySummaryModalProps } from "./colony/DeploySummaryModal";
export { DriftBadge } from "./colony/DriftBadge";
export type { DriftBadgeProps } from "./colony/DriftBadge";
export { CronFrequencyChip } from "./colony/CronFrequencyChip";
export type { CronFrequencyChipProps } from "./colony/CronFrequencyChip";
export { WorkspaceTargetPicker } from "./colony/WorkspaceTargetPicker";
export type { WorkspaceTargetPickerProps } from "./colony/WorkspaceTargetPicker";
export { EventRibbon } from "./colony/EventRibbon";
export { SafetyPanel } from "./colony/SafetyPanel";
export type { SafetyPanelProps } from "./colony/SafetyPanel";
export { RuleBuilderOverlay } from "./colony/RuleBuilderOverlay";
export type { RuleBuilderOverlayProps } from "./colony/RuleBuilderOverlay";
export { TopBar } from "./colony/TopBar";
export { Viewport } from "./colony/Viewport";
export { ColonyCanvas } from "./colony/ColonyCanvas";
export { BottomPanel } from "./colony/BottomPanel";

// Colony types + utilities
export type {
  ColonyNode, ColonyAgent, ColonyConnection, ColonyFormation,
  ColonySelection, ColonyCommand, ColonyPipe, DetailView,
} from "./colony/types";
export { seeded, hash, COMMANDS, NODE_TYPES, MOMENTUM_NAMES, MOMENTUM_COLORS } from "./colony/types";

// Colony geometry (shared position calculations for canvas + minimap)
export { getConnectorPosition, getAgentPosition, getFormationAgents, getFormationBounds } from "./colony/geometry";
export type { ConnectorPositions } from "./colony/geometry";

// Colony data mappers (DashboardState → colony visual model)
export { mapNodes, mapAgents, mapFormations } from "./colony/mappers";

// Colony panels
export { AiConfigPanel } from "./colony/AiConfigPanel";
export { AppSettingsPanel } from "./colony/AppSettingsPanel";
export { ConnectorConfigPanel } from "./colony/ConnectorConfigPanel";
export { TeamBuilder } from "./colony/TeamBuilder";
export type { TeamConfig, TeamBuilderSeed } from "./colony/TeamBuilder";

// Dashboard state (data layer — platform-agnostic)
export { createDashboardState, DashboardProvider, useDashboard } from "./dashboard/context";
export type { DataProvider, DashboardState, FormationInfo, FormationMemberDetail, FormationDetail, RuleSummary, ConfigSchema, ConfigSchemaProperty, AvailableConnector, CommandDecl, MemberRef, CooperationEvent, CooperationEventEnvelope, AgentHealthDetail, Recipe, RecipeCategory, RecipeFilter, RecipeSort, RecipeSource, RecipeSourceFilter, Difficulty, InputField, FieldKind, SelectOption, RecipeInputs, RecipeApplyReport, PreflightReport, PreflightItem, PreflightStatus, PreflightFix, PreviewReport, PreviewStep, RecipePiece, RecipePieceSummary, SafetyConfig } from "./dashboard/types";
export { createProviderQuery, createProviderMutation } from "./dashboard/query";
export type { MutationResult } from "./dashboard/query";

// Form components (rendered inside colony panels)
export { ConditionEditor } from "./ConditionEditor";
export type { ConditionDef } from "./ConditionEditor";
export { TriggerPicker } from "./TriggerPicker";
export { ActionPicker } from "./ActionPicker";
export { RulePreview } from "./RulePreview";

// A2UI / Canvas block renderer — generic structured-output surface
// the bot pushes content to. Distinct from `ColonyCanvas` which
// visualises cooperation infrastructure.
export { Canvas } from "./Canvas";
export type { CanvasProps } from "./Canvas";

// Dashboard data model types
export type { ConnectorStatus, RuleItem, RuleDetail, EventItem, SwarmInfo } from "./dashboard/model";
