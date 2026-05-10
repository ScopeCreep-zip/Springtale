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
export { MemberPickerOverlay } from "./colony/MemberPickerOverlay";
export type { MemberPickerOverlayProps } from "./colony/MemberPickerOverlay";
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
export type { TeamConfig } from "./colony/TeamBuilder";

// Dashboard state (data layer — platform-agnostic)
export { createDashboardState, DashboardProvider, useDashboard } from "./dashboard/context";
export type { DataProvider, DashboardState, FormationInfo, FormationMemberDetail, FormationDetail, RuleSummary, ConfigSchema, ConfigSchemaProperty, AvailableConnector, CommandDecl, MemberRef } from "./dashboard/types";
export { createProviderQuery, createProviderMutation } from "./dashboard/query";
export type { MutationResult } from "./dashboard/query";

// Form components (rendered inside colony panels)
export { ConditionEditor } from "./ConditionEditor";
export type { ConditionDef } from "./ConditionEditor";
export { TriggerPicker } from "./TriggerPicker";
export { ActionPicker } from "./ActionPicker";
export { RulePreview } from "./RulePreview";

// Dashboard data model types
export type { ConnectorStatus, RuleItem, RuleDetail, EventItem, SwarmInfo } from "./dashboard/model";
