/**
 * @springtale/ui — Shared SolidJS component library.
 *
 * Used by both the Tauri desktop app and the web dashboard.
 * Components render state and forward actions — no business logic.
 *
 * Colony ecosystem visualization: trees=connectors, springtails=agents,
 * mycelium=pipelines. Pixel-art aesthetic via Silkscreen font + box-shadow sprites.
 */

export { createI18n, I18nProvider, useI18n } from "./i18n/context";
export type { Locale, RawDictionary } from "./i18n/types";

// Colony layout components
export { ColonyShell } from "./colony/ColonyShell";
export { TopBar } from "./colony/TopBar";
export { Viewport } from "./colony/Viewport";
export { ColonyCanvas } from "./colony/ColonyCanvas";
export { BottomPanel } from "./colony/BottomPanel";

// Colony types + utilities
export type {
  ColonyTree, ColonyAgent, ColonyConnection, ColonyFormation,
  ColonySelection, ColonyCommand, ColonyPipe,
} from "./colony/types";
export { seeded, hash, COMMANDS, TREE_TYPES, MOMENTUM_NAMES, MOMENTUM_COLORS } from "./colony/types";

// Dashboard state (data layer — platform-agnostic)
export { createDashboardState, DashboardProvider, useDashboard } from "./dashboard/context";
export type { DataProvider, DashboardState, FormationInfo, RuleSummary } from "./dashboard/types";

// Form components (rendered inside colony detail panel)
export { TriggerPicker } from "./TriggerPicker";
export { ActionPicker } from "./ActionPicker";
export { ConditionEditor } from "./ConditionEditor";
export type { ConditionDef } from "./ConditionEditor";
export { RulePreview } from "./RulePreview";
export { Canvas } from "./Canvas";
export { HatchWizard } from "./HatchWizard";

// Types still needed by dashboard state internals
export type { EventItem } from "./CommandPanel";
export type { ConnectorStatus } from "./ResourceBar";
export type { RuleItem } from "./Roster";
export type { RuleDetail } from "./CommandPanel";
export type { SwarmInfo } from "./SwarmCard";
