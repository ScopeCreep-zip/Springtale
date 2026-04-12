/**
 * Desktop data provider — wraps Tauri IPC modules into DataProvider interface.
 *
 * Thin adapter: no logic, just maps existing ipc/ functions to the
 * platform-agnostic DataProvider that createDashboardState() expects.
 */
import type { DataProvider } from "@springtale/ui";
import { listen } from "@tauri-apps/api/event";
import type { EventEntry, CanvasUpdate } from "@springtale/types";

import { listConnectors, listAvailableConnectors, setupConnector, getConnectorSchemas, enableConnector, disableConnector, removeConnector, removeConnectorCascade, getConnectorConfig, listConnectorOutputs } from "./ipc/connectors";
import { listRules, createRule, toggleRule, deleteRule, updateRule, runRule, parseRuleFromIntent, listRulesForConnector, testConnector, reassignRuleConnector, createConnectorRule, getRuleSchema } from "./ipc/rules";
import { listEvents } from "./ipc/events";
import {
  listFormations,
  createFormation,
  deployFormation,
  pauseFormation,
  resumeFormation,
  dissolveFormation,
  updateFormationIntent,
  addFormationMember,
  listIntents,
  deployTeam,
  cycleFormationIntent,
  cycleFormationAutonomy,
} from "./ipc/formations";
import { getCanvasState, getConnections } from "./ipc/canvas";
import {
  getConfig, setConfig, listConfig, setAiAdapter, setConnectorConfig,
  configureAiAdapter, upsertConnectorConfig, toggleFormationGuard,
  exportData, auditMemory, compactMemory,
} from "./ipc/config";
import { listAgentStates, getAutonomy, setAutonomy, stepAutonomy } from "./ipc/agents";
import { listAuthors, addAuthor, removeAuthor } from "./ipc/authors";

export function createDesktopProvider(): DataProvider {
  return {
    // Connectors
    listConnectors,
    listAvailableConnectors,
    setupConnector: (name: string, config: Record<string, unknown>) => setupConnector(name, config),
    getConnectorSchemas,
    enableConnector: (name: string) => enableConnector(name),
    disableConnector: (name: string) => disableConnector(name),
    removeConnector: (name: string) => removeConnector(name),
    removeConnectorCascade: (name: string) => removeConnectorCascade(name),
    getConnectorConfig: (name: string) => getConnectorConfig(name),
    listConnectorOutputs: (name: string, limit?: number) => listConnectorOutputs(name, limit) as any,

    // Rules
    listRules,
    createRule,
    toggleRule,
    deleteRule,
    updateRule,
    runRule,
    parseRuleFromIntent,
    createConnectorRule,
    getRuleSchema,
    listRulesForConnector,
    testConnector,
    reassignRuleConnector,

    // Events
    listEvents,
    subscribeToEvents(callback) {
      let unlisten: (() => void) | undefined;
      listen<EventEntry>("event-fired", (e) => callback(e.payload))
        .then((u) => { unlisten = u; })
        .catch(() => {});
      return () => unlisten?.();
    },

    // Formations
    listFormations,
    createFormation,
    deployFormation,
    pauseFormation,
    resumeFormation,
    dissolveFormation,
    updateFormationIntent,
    addFormationMember,
    listIntents,
    deployTeam,
    cycleFormationIntent,
    cycleFormationAutonomy,

    // Config
    getConfig,
    setConfig,
    listConfig,
    setAiAdapter,
    setConnectorConfig,
    configureAiAdapter,
    upsertConnectorConfig,
    toggleFormationGuard,

    // Agent state + autonomy
    listAgentStates,
    getAutonomy,
    setAutonomy,
    stepAutonomy,

    // Trusted authors
    listAuthors,
    addAuthor,
    removeAuthor,

    // Data
    exportData,

    // Memory
    auditMemory,
    compactMemory,

    // Canvas
    getConnections,
    getCanvasState,
    subscribeToCanvasUpdates(callback) {
      let unlisten: (() => void) | undefined;
      listen<CanvasUpdate>("canvas-update", (e) => callback(e.payload))
        .then((u) => { unlisten = u; })
        .catch(() => {});
      return () => unlisten?.();
    },
  };
}
